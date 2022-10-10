// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

//! Program state processor

use std::ops::{Add, Sub};

use crate::{
    error::LidoError,
    instruction::{
        DepositAccountsInfo, InitializeAccountsInfo, LidoInstruction, MigrateStateToV2Info,
        StakeDepositAccountsInfoV2, UnstakeAccountsInfoV2, UpdateExchangeRateAccountsInfoV2,
        UpdateStakeAccountBalanceInfo, WithdrawAccountsInfoV2,
    },
    logic::{
        burn_st_sol, check_account_data, check_account_owner, check_mint, check_rent_exempt,
        check_unstake_accounts, create_account_even_if_funded, distribute_fees,
        initialize_stake_account_undelegated, mint_st_sol_to, split_stake_account,
        transfer_stake_authority, CreateAccountOptions, SplitStakeAccounts,
    },
    metrics::Metrics,
    process_management::{
        process_add_maintainer, process_add_validator, process_change_reward_distribution,
        process_deactivate_validator, process_deactivate_validator_if_commission_exceeds_max,
        process_merge_stake, process_remove_maintainer, process_remove_validator,
        process_set_max_commission_percentage,
    },
    stake_account::{deserialize_stake_account, StakeAccount},
    state::{
        AccountType, ExchangeRate, FeeRecipients, Lido, LidoV1, ListEntry, Maintainer,
        MaintainerList, RewardDistribution, StakeDeposit, Validator, ValidatorList,
    },
    token::{Lamports, Rational, StLamports},
    MAXIMUM_UNSTAKE_ACCOUNTS, MINIMUM_STAKE_ACCOUNT_BALANCE, MINT_AUTHORITY, RESERVE_ACCOUNT,
    STAKE_AUTHORITY, VALIDATOR_STAKE_ACCOUNT, VALIDATOR_UNSTAKE_ACCOUNT,
};

use solana_program::stake as stake_program;
use solana_program::stake_history::StakeHistory;
use {
    borsh::BorshDeserialize,
    solana_program::{
        account_info::AccountInfo,
        clock::Clock,
        entrypoint::ProgramResult,
        msg,
        native_token::LAMPORTS_PER_SOL,
        program::{invoke, invoke_signed},
        program_error::ProgramError,
        pubkey::Pubkey,
        rent::Rent,
        system_instruction,
        sysvar::Sysvar,
    },
};

/// Program state handler.
pub fn process_initialize(
    program_id: &Pubkey,
    reward_distribution: RewardDistribution,
    max_validators: u32,
    max_maintainers: u32,
    max_commission_percentage: u8,
    accounts_raw: &[AccountInfo],
) -> ProgramResult {
    let accounts = InitializeAccountsInfo::try_from_slice(accounts_raw)?;
    let rent = &Rent::get()?;
    check_rent_exempt(rent, accounts.lido, "Solido account")?;
    check_rent_exempt(rent, accounts.reserve_account, "Reserve account")?;
    check_rent_exempt(rent, accounts.validator_list, "Validator list account")?;
    check_rent_exempt(rent, accounts.maintainer_list, "Maintainer list account")?;

    check_account_owner(accounts.lido, program_id)?;
    check_account_owner(accounts.validator_list, program_id)?;
    check_account_owner(accounts.maintainer_list, program_id)?;

    check_account_data(accounts.lido, Lido::LEN, AccountType::Lido)?;
    check_account_data(
        accounts.validator_list,
        ValidatorList::required_bytes(max_validators),
        AccountType::Validator,
    )?;
    check_account_data(
        accounts.maintainer_list,
        MaintainerList::required_bytes(max_maintainers),
        AccountType::Maintainer,
    )?;

    if accounts.lido.key == accounts.validator_list.key {
        msg!("Cannot use same account for Lido and validator list");
        return Err(LidoError::AlreadyInUse.into());
    }
    if accounts.lido.key == accounts.maintainer_list.key {
        msg!("Cannot use same account for Lido and maintainer list");
        return Err(LidoError::AlreadyInUse.into());
    }
    if accounts.validator_list.key == accounts.maintainer_list.key {
        msg!("Cannot use same account for validator list and maintainer list");
        return Err(LidoError::AlreadyInUse.into());
    }

    let mut validators = ValidatorList::new_default(0);
    validators.header.max_entries = max_validators;

    let mut maintainers = MaintainerList::new_default(0);
    maintainers.header.max_entries = max_maintainers;

    let (reserve_account_pda, reserve_bump_seed) = Pubkey::find_program_address(
        &[&accounts.lido.key.to_bytes(), RESERVE_ACCOUNT],
        program_id,
    );
    if &reserve_account_pda != accounts.reserve_account.key {
        msg!(
            "Resrve account {} is incorrect, should be {}",
            accounts.reserve_account.key,
            reserve_account_pda
        );
        return Err(LidoError::IncorrectReserveAddress.into());
    }

    let (_, stake_bump_seed) = Pubkey::find_program_address(
        &[&accounts.lido.key.to_bytes(), STAKE_AUTHORITY],
        program_id,
    );

    let (mint_authority, mint_bump_seed) =
        Pubkey::find_program_address(&[&accounts.lido.key.to_bytes(), MINT_AUTHORITY], program_id);
    // Check if the token has no minted tokens and right mint authority.
    check_mint(rent, accounts.st_sol_mint, &mint_authority)?;

    if max_commission_percentage > 100 {
        return Err(LidoError::ValidationCommissionOutOfBounds.into());
    }

    // Initialize fee structure
    let lido = Lido {
        lido_version: Lido::VERSION,
        account_type: AccountType::Lido,
        manager: *accounts.manager.key,
        st_sol_mint: *accounts.st_sol_mint.key,
        exchange_rate: ExchangeRate::default(),
        sol_reserve_account_bump_seed: reserve_bump_seed,
        mint_authority_bump_seed: mint_bump_seed,
        stake_authority_bump_seed: stake_bump_seed,
        reward_distribution,
        fee_recipients: FeeRecipients {
            treasury_account: *accounts.treasury_account.key,
            developer_account: *accounts.developer_account.key,
        },
        metrics: Metrics::new(),
        validator_list: *accounts.validator_list.key,
        maintainer_list: *accounts.maintainer_list.key,
        max_commission_percentage,
    };

    // Confirm that the fee recipients are actually stSOL accounts.
    lido.check_is_st_sol_account(accounts.treasury_account)?;
    lido.check_is_st_sol_account(accounts.developer_account)?;

    validators.save(accounts.validator_list)?;
    maintainers.save(accounts.maintainer_list)?;
    lido.save(accounts.lido)
}

pub fn process_deposit(
    program_id: &Pubkey,
    amount: Lamports,
    accounts_raw: &[AccountInfo],
) -> ProgramResult {
    let accounts = DepositAccountsInfo::try_from_slice(accounts_raw)?;

    if amount == Lamports(0) {
        msg!("Amount must be greater than zero");
        return Err(ProgramError::InvalidArgument);
    }

    let mut lido = Lido::deserialize_lido(program_id, accounts.lido)?;
    lido.check_reserve_account(program_id, accounts.lido.key, accounts.reserve_account)?;

    invoke(
        &system_instruction::transfer(accounts.user.key, accounts.reserve_account.key, amount.0),
        &[
            accounts.user.clone(),
            accounts.reserve_account.clone(),
            accounts.system_program.clone(),
        ],
    )?;

    let st_sol_amount = lido.exchange_rate.exchange_sol(amount)?;

    mint_st_sol_to(
        &lido,
        accounts.lido.key,
        accounts.spl_token,
        accounts.st_sol_mint,
        accounts.mint_authority,
        accounts.recipient,
        st_sol_amount,
    )?;

    // Explain what we did in the logs, because block explorers can be an
    // inscrutable mess of accounts, especially without special parsers for
    // Solido transactions. With the logs, we can still identify what happened.
    msg!(
        "Solido: Deposited {}, minted {} in return.",
        amount,
        st_sol_amount
    );

    lido.metrics.deposit_amount.observe(amount)?;
    lido.save(accounts.lido)
}

pub fn process_stake_deposit(
    program_id: &Pubkey,
    amount: Lamports,
    validator_index: u32,
    maintainer_index: u32,
    raw_accounts: &[AccountInfo],
) -> ProgramResult {
    let accounts = StakeDepositAccountsInfoV2::try_from_slice(raw_accounts)?;

    let lido = Lido::deserialize_lido(program_id, accounts.lido)?;

    lido.check_maintainer(
        program_id,
        accounts.maintainer_list,
        maintainer_index,
        accounts.maintainer,
    )?;
    lido.check_reserve_account(program_id, accounts.lido.key, accounts.reserve)?;
    lido.check_stake_authority(program_id, accounts.lido.key, accounts.stake_authority)?;
    lido.check_can_stake_amount(accounts.reserve, amount)?;

    let validator_list_data = &mut *accounts.validator_list.data.borrow_mut();
    let mut validators = lido.deserialize_account_list_info::<Validator>(
        program_id,
        accounts.validator_list,
        validator_list_data,
    )?;

    // Confirm that there is no other active validator with a lower balance that
    // we could stake to. This alone is not sufficient to guarantee a uniform
    // stake balance, but it limits the power that maintainers have to disturb
    // the balance. More importantly, it ensures that when two maintainers create
    // the same StakeDeposit transaction, only one of them succeeds.
    let minimum_stake_validator = validators
        .iter()
        .filter(|&v| v.active)
        .min_by_key(|v| v.effective_stake_balance)
        .ok_or(LidoError::NoActiveValidators)?;
    let minimum_stake_pubkey = *minimum_stake_validator.pubkey();
    let minimum_stake_balance = minimum_stake_validator.effective_stake_balance;

    let validator = validators.get_mut(validator_index, accounts.validator_vote_account.key)?;

    if !validator.active {
        msg!(
            "Validator {} is inactive, new deposits are not allowed",
            validator.pubkey()
        );
        return Err(LidoError::StakeToInactiveValidator.into());
    }

    // Note that we compare balances, not keys, because the minimum might not be unique.
    if validator.effective_stake_balance > minimum_stake_balance {
        msg!(
            "Refusing to stake with {}, who has {} stake, \
            because {} has less stake: {}. Stake there instead.",
            validator.pubkey(),
            validator.effective_stake_balance,
            minimum_stake_pubkey,
            minimum_stake_balance,
        );
        return Err(LidoError::ValidatorWithLessStakeExists.into());
    }

    let clock = Clock::get()?;

    // Now we have two options:
    //
    // 1. This was the first time we stake in this epoch, so we cannot merge the
    //    new account into anything. We need to delegate it, and "consume" the
    //    new stake account at this seed.
    //
    // 2. There already exists an activating stake account for the validator,
    //    and we can merge into it. The number of stake accounts does not change.
    //
    // We assume that the maintainer checked this, and we are in case 2 if the
    // accounts passed differ, and in case 1 if they don't. Note, if the
    // maintainer incorrectly opted for merge, the transaction will fail. If the
    // maintainer incorrectly opted for append, we will consume one stake account
    // that could have been avoided, but it can still be merged after activation.
    let (approach, stake_account_end_authority) =
        if accounts.stake_account_end.key == accounts.stake_account_merge_into.key {
            (StakeDeposit::Append, VALIDATOR_STAKE_ACCOUNT.to_vec())
        } else {
            (
                StakeDeposit::Merge,
                // stake_account_end should be destroyed after a transaction, but a malicious
                // maintainer could append an instruction to the transaction that
                // transfers some SOL to this account and changes stake/withdraw authority thus making
                // it a permanent account. This will make stake deposit fail. We create a temporary
                // stake account tied to the current epoch so that stake account reviving could
                // affect only the current epoch. And stake deposit should work in the next epoch after
                // we remove the maintainer from Solido
                [VALIDATOR_STAKE_ACCOUNT, &clock.epoch.to_le_bytes()[..]].concat(),
            )
        };

    let stake_account_bump_seed = Lido::check_stake_account(
        program_id,
        accounts.lido.key,
        validator,
        validator.stake_seeds.end,
        accounts.stake_account_end,
        &stake_account_end_authority,
    )?;

    if accounts.stake_account_end.data.borrow().len() > 0 {
        msg!(
            "Stake account {} contains data, aborting.",
            accounts.stake_account_end.key
        );
        return Err(LidoError::WrongStakeState.into());
    }

    let stake_account_seed = validator.stake_seeds.end.to_le_bytes();
    let stake_account_bump_seed = [stake_account_bump_seed];
    let stake_account_seeds = &[
        accounts.lido.key.as_ref(),
        validator.vote_account_address.as_ref(),
        &stake_account_end_authority,
        &stake_account_seed[..],
        &stake_account_bump_seed[..],
    ][..];

    // Create the account that is going to hold the new stake account data.
    // Even if it was already funded.
    create_account_even_if_funded(
        accounts.lido.key,
        CreateAccountOptions {
            fund_amount: amount,
            data_size: std::mem::size_of::<stake_program::state::StakeState>() as u64,
            owner: stake_program::program::id(),
            sign_seeds: stake_account_seeds,
            account: accounts.stake_account_end,
        },
        accounts.reserve,
        lido.sol_reserve_account_bump_seed,
        accounts.system_program,
    )?;

    // Now initialize the stake, but do not yet delegate it.
    initialize_stake_account_undelegated(
        accounts.stake_authority.key,
        accounts.stake_account_end,
        accounts.sysvar_rent,
        accounts.stake_program,
    )?;

    // Update the amount staked for this validator. Note that it could happen
    // that there is now more SOL in the account than what we put in there, if
    // someone deposited into the account before we started using it. We don't
    // record that here; we will discover it later in `WithdrawInactiveStake`,
    // and then it will be treated as a donation.
    msg!("Staked {} out of the reserve.", amount);
    validator.stake_accounts_balance = (validator.stake_accounts_balance + amount)?;
    validator.effective_stake_balance = validator.compute_effective_stake_balance();

    match approach {
        // Case 1: we delegate, and we don't touch `stake_account_merge_into`.
        StakeDeposit::Append => {
            msg!(
                "Delegating stake account at seed {} ...",
                validator.stake_seeds.end
            );
            invoke_signed(
                &stake_program::instruction::delegate_stake(
                    accounts.stake_account_end.key,
                    accounts.stake_authority.key,
                    accounts.validator_vote_account.key,
                ),
                &[
                    accounts.stake_account_end.clone(),
                    accounts.validator_vote_account.clone(),
                    accounts.sysvar_clock.clone(),
                    accounts.stake_history.clone(),
                    accounts.stake_program_config.clone(),
                    accounts.stake_authority.clone(),
                    accounts.stake_program.clone(),
                ],
                &[&[
                    accounts.lido.key.as_ref(),
                    STAKE_AUTHORITY,
                    &[lido.stake_authority_bump_seed],
                ]],
            )?;

            // We now consumed this stake account, bump the index.
            validator.stake_seeds.end += 1;
        }
        StakeDeposit::Merge => {
            // Case 2: Merge the new undelegated stake account into the existing one.
            if validator.stake_seeds.end <= validator.stake_seeds.begin {
                msg!("Can only stake-merge if there is at least one stake account to merge into.");
                return Err(LidoError::InvalidStakeAccount.into());
            }
            Lido::check_stake_account(
                program_id,
                accounts.lido.key,
                validator,
                // Does not underflow, because end > begin >= 0.
                validator.stake_seeds.end - 1,
                accounts.stake_account_merge_into,
                VALIDATOR_STAKE_ACCOUNT,
            )?;
            // The stake program checks that the two accounts can be merged; if we
            // tried to merge, but the epoch is different, then this will fail.
            msg!(
                "Merging into existing stake account at seed {} ...",
                validator.stake_seeds.end - 1
            );
            let merge_instructions = stake_program::instruction::merge(
                accounts.stake_account_merge_into.key,
                accounts.stake_account_end.key,
                accounts.stake_authority.key,
            );
            // For some reason, `merge` returns a `Vec` of instructions, but when
            // you look at the implementation, it unconditionally returns a single
            // instruction.
            assert_eq!(merge_instructions.len(), 1);
            let merge_instruction = &merge_instructions[0];

            invoke_signed(
                merge_instruction,
                &[
                    accounts.stake_account_merge_into.clone(),
                    accounts.stake_account_end.clone(),
                    accounts.sysvar_clock.clone(),
                    accounts.stake_history.clone(),
                    accounts.stake_authority.clone(),
                    accounts.stake_program.clone(),
                ],
                &[&[
                    accounts.lido.key.as_ref(),
                    STAKE_AUTHORITY,
                    &[lido.stake_authority_bump_seed],
                ]],
            )?;
        }
    };

    Ok(())
}

/// Unstakes from a validator, the funds are moved to the stake defined by the
/// validator's unstake seed. Caller must be a maintainer.
pub fn process_unstake(
    program_id: &Pubkey,
    amount: Lamports,
    validator_index: u32,
    maintainer_index: u32,
    raw_accounts: &[AccountInfo],
) -> ProgramResult {
    let accounts = UnstakeAccountsInfoV2::try_from_slice(raw_accounts)?;
    let lido = Lido::deserialize_lido(program_id, accounts.lido)?;
    lido.check_maintainer(
        program_id,
        accounts.maintainer_list,
        maintainer_index,
        accounts.maintainer,
    )?;

    lido.check_stake_authority(program_id, accounts.lido.key, accounts.stake_authority)?;

    let validator_list_data = &mut *accounts.validator_list.data.borrow_mut();
    let mut validators = lido.deserialize_account_list_info::<Validator>(
        program_id,
        accounts.validator_list,
        validator_list_data,
    )?;

    let validator = validators.get_mut(validator_index, accounts.validator_vote_account.key)?;
    let destination_bump_seed = check_unstake_accounts(program_id, validator, &accounts)?;

    // Because `WithdrawInactiveStake` needs to reference all stake and unstake
    // accounts in a single transaction, we shouldn't have too many of them.
    // We should only need to do one unstake per epoch, right at the end, and in
    // the next epoch it should be fully inactive, we withdraw it and bump the
    // seed, and then we can unstake again.
    if validator.unstake_seeds.end - validator.unstake_seeds.begin >= MAXIMUM_UNSTAKE_ACCOUNTS {
        msg!("This validator already has 3 unstake accounts.");
        msg!("Please wait until the next epoch and withdraw them, then try to unstake again.");
        return Err(LidoError::MaxUnstakeAccountsReached.into());
    }

    let seeds = [
        &accounts.lido.key.to_bytes(),
        &accounts.validator_vote_account.key.to_bytes(),
        VALIDATOR_UNSTAKE_ACCOUNT,
        &validator.unstake_seeds.end.to_le_bytes()[..],
        &[destination_bump_seed],
    ];

    let source_balance = Lamports(accounts.source_stake_account.lamports());

    split_stake_account(
        accounts.lido.key,
        &lido,
        &SplitStakeAccounts {
            source_stake_account: accounts.source_stake_account,
            destination_stake_account: accounts.destination_unstake_account,
            authority: accounts.stake_authority,
            system_program: accounts.system_program,
            stake_program: accounts.stake_program,
        },
        amount,
        &[&seeds],
    )?;

    let deactivate_stake_instruction = solana_program::stake::instruction::deactivate_stake(
        accounts.destination_unstake_account.key,
        accounts.stake_authority.key,
    );

    // Deactivates the stake. After the stake has become inactive, the Lamports
    // on this stake account need to go back to the reserve account by using
    // another instruction.
    invoke_signed(
        &deactivate_stake_instruction,
        &[
            accounts.destination_unstake_account.clone(),
            accounts.sysvar_clock.clone(),
            accounts.stake_authority.clone(),
            accounts.stake_program.clone(),
        ],
        &[&[
            &accounts.lido.key.to_bytes(),
            STAKE_AUTHORITY,
            &[lido.stake_authority_bump_seed],
        ]],
    )?;

    if validator.active {
        // For active validators, we don't allow their stake accounts to contain
        // less than the minimum stake account balance.
        let new_source_balance = (source_balance - amount)?;
        if new_source_balance < MINIMUM_STAKE_ACCOUNT_BALANCE {
            msg!(
                "Unstake operation will leave the stake account with {}, less \
                than the minimum balance {}. Only inactive validators can fall \
                below the limit.",
                new_source_balance,
                MINIMUM_STAKE_ACCOUNT_BALANCE
            );
            return Err(LidoError::InvalidAmount.into());
        }
    } else {
        // For inactive validators on the other hand, we only allow unstaking
        // the full stake account, so we can decrease the stake as quickly as
        // possible. This leaves the source account empty, so we bump the seed.
        if amount != source_balance {
            msg!(
                "An inactive validator must the full stake account withdrawn. \
                Tried to withdraw {}, should withdraw {} instead.",
                amount,
                source_balance,
            );
            return Err(LidoError::InvalidAmount.into());
        }
        validator.stake_seeds.begin += 1;
    }

    validator.unstake_accounts_balance = (validator.unstake_accounts_balance + amount)?;
    validator.effective_stake_balance = validator.compute_effective_stake_balance();
    validator.unstake_seeds.end += 1;

    Ok(())
}

pub fn process_update_exchange_rate(
    program_id: &Pubkey,
    raw_accounts: &[AccountInfo],
) -> ProgramResult {
    let accounts = UpdateExchangeRateAccountsInfoV2::try_from_slice(raw_accounts)?;
    let mut lido = Lido::deserialize_lido(program_id, accounts.lido)?;
    lido.check_reserve_account(program_id, accounts.lido.key, accounts.reserve)?;

    let clock = Clock::get()?;
    let rent = Rent::get()?;

    if lido.exchange_rate.computed_in_epoch >= clock.epoch {
        msg!(
            "The exchange rate was already updated in epoch {}.",
            lido.exchange_rate.computed_in_epoch
        );
        msg!("It can only be done once per epoch, so we are going to abort this transaction.");
        return Err(LidoError::ExchangeRateAlreadyUpToDate.into());
    }

    let validator_list_data = &mut *accounts.validator_list.data.borrow_mut();
    let validators = lido.deserialize_account_list_info::<Validator>(
        program_id,
        accounts.validator_list,
        validator_list_data,
    )?;

    lido.exchange_rate.computed_in_epoch = clock.epoch;
    lido.exchange_rate.sol_balance =
        Lido::get_sol_balance(validators.iter(), &rent, accounts.reserve)?;
    lido.exchange_rate.st_sol_supply = lido.get_st_sol_supply(accounts.st_sol_mint)?;

    lido.save(accounts.lido)
}

#[derive(PartialEq, Clone, Copy)]
pub enum StakeType {
    Stake,
    Unstake,
}

impl std::fmt::Display for StakeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StakeType::Stake => write!(f, "stake"),
            StakeType::Unstake => write!(f, "unstake"),
        }
    }
}

pub struct WithdrawExcessOpts<'a, 'b> {
    accounts: &'a UpdateStakeAccountBalanceInfo<'a, 'b>,
    clock: &'a Clock,
    stake_history: &'a StakeHistory,
    stake_account: &'a AccountInfo<'b>,
    stake_account_seed: u64,
    stake_authority_bump_seed: u8,
}

/// Withdraw `amount` from `withdraw_excess_opts.stake_account`.
pub fn withdraw_inactive_sol(
    withdraw_excess_opts: &WithdrawExcessOpts,
    amount: Lamports,
) -> Result<(), ProgramError> {
    if amount == Lamports(0) {
        return Ok(());
    }
    let withdraw_instruction = StakeAccount::stake_account_withdraw(
        amount,
        withdraw_excess_opts.stake_account.key,
        withdraw_excess_opts.accounts.reserve.key,
        withdraw_excess_opts.accounts.stake_authority.key,
    );
    invoke_signed(
        &withdraw_instruction,
        &[
            withdraw_excess_opts.stake_account.clone(),
            withdraw_excess_opts.accounts.reserve.clone(),
            withdraw_excess_opts.accounts.sysvar_clock.clone(),
            withdraw_excess_opts.accounts.sysvar_stake_history.clone(),
            withdraw_excess_opts.accounts.stake_authority.clone(),
            withdraw_excess_opts.accounts.stake_program.clone(),
        ],
        &[&[
            withdraw_excess_opts.accounts.lido.key.as_ref(),
            STAKE_AUTHORITY,
            &[withdraw_excess_opts.stake_authority_bump_seed],
        ]],
    )?;
    msg!(
        "Withdrew {} inactive stake back to the reserve from stake account at seed {}.",
        amount,
        withdraw_excess_opts.stake_account_seed
    );

    Ok(())
}

pub fn get_stake_account(
    withdraw_excess_opts: &WithdrawExcessOpts,
) -> Result<StakeAccount, ProgramError> {
    let stake = deserialize_stake_account(&withdraw_excess_opts.stake_account.data.borrow())?;
    Ok(StakeAccount::from_delegated_account(
        Lamports(withdraw_excess_opts.stake_account.lamports()),
        &stake,
        withdraw_excess_opts.clock,
        withdraw_excess_opts.stake_history,
        withdraw_excess_opts.stake_account_seed,
    ))
}

/// Checks that the `derived_stake_account_address` corresponds to the
/// `provided_stake_account`. Returns the number of Lamports in the stake
/// account or errors if the derived address is different.
pub fn check_address_and_get_balance(
    derived_stake_account_address: &Pubkey,
    provided_stake_account: &AccountInfo,
    stake_account_seed: u64,
    stake_type: StakeType,
) -> Result<Lamports, LidoError> {
    if provided_stake_account.key != derived_stake_account_address {
        msg!(
            "Wrong {} account provided for seed {}: expected {} but got {}.",
            stake_type,
            stake_account_seed,
            derived_stake_account_address,
            provided_stake_account.key,
        );
        return Err(LidoError::InvalidStakeAccount);
    }
    let account_balance = Lamports(**provided_stake_account.lamports.borrow());
    msg!(
        "{} account at seed {} ({}) contains {}.",
        stake_type,
        stake_account_seed,
        provided_stake_account.key,
        account_balance,
    );
    Ok(account_balance)
}

/// Recover any inactive balance from the validator's stake/unstake accounts to
/// the reserve account.
/// Updates the validator's balance and distribute rewards.
/// This function is permissionless and can be called by anyone.
pub fn process_update_stake_account_balance(
    program_id: &Pubkey,
    validator_index: u32,
    raw_accounts: &[AccountInfo],
) -> ProgramResult {
    let accounts = UpdateStakeAccountBalanceInfo::try_from_slice(raw_accounts)?;
    let mut lido = Lido::deserialize_lido(program_id, accounts.lido)?;
    let stake_history = StakeHistory::from_account_info(accounts.sysvar_stake_history)?;
    let clock = Clock::get()?;
    let rent = Rent::get()?;

    // Confirm that the passed accounts are the ones configured in the state,
    // and confirm that they can receive stSOL.
    lido.check_reserve_account(program_id, accounts.lido.key, accounts.reserve)?;

    let validator_list_data = &mut *accounts.validator_list.data.borrow_mut();
    let mut validators = lido.deserialize_account_list_info::<Validator>(
        program_id,
        accounts.validator_list,
        validator_list_data,
    )?;

    let validator = validators.get_mut(validator_index, accounts.validator_vote_account.key)?;

    let mut stake_observed_total = Lamports(0);
    let mut excess_removed = Lamports(0);
    let n_stake_accounts = validator.stake_seeds.end - validator.stake_seeds.begin;
    let n_unstake_accounts = validator.unstake_seeds.end - validator.unstake_seeds.begin;

    if accounts.stake_accounts.len() as u64 != n_stake_accounts + n_unstake_accounts {
        msg!("Wrong number of stake accounts provided, expected {} stake accounts and {} unstake accounts, \
            but got {} accounts.", n_stake_accounts, n_unstake_accounts, accounts.stake_accounts.len());
        return Err(LidoError::InvalidStakeAccount.into());
    }
    // Does not panic, because len = n_stake_accounts + n_unstake_accounts >= n_stake_accounts.
    let (stake_accounts, unstake_accounts) =
        accounts.stake_accounts.split_at(n_stake_accounts as usize);

    // Visit the stake accounts one by one, and check how much SOL is in there.
    for (seed, provided_stake_account) in
        validator.stake_seeds.into_iter().zip(stake_accounts.iter())
    {
        let (stake_account_address, _bump_seed) = validator.find_stake_account_address(
            program_id,
            accounts.lido.key,
            seed,
            StakeType::Stake,
        );
        let account_balance = check_address_and_get_balance(
            &stake_account_address,
            provided_stake_account,
            seed,
            StakeType::Stake,
        )?;
        let withdraw_opts = WithdrawExcessOpts {
            accounts: &accounts,
            clock: &clock,
            stake_history: &stake_history,
            stake_account: provided_stake_account,
            stake_account_seed: seed,
            stake_authority_bump_seed: lido.stake_authority_bump_seed,
        };

        let stake_account = get_stake_account(&withdraw_opts)?;
        let amount = (stake_account.balance.inactive
            - Lamports(rent.minimum_balance(provided_stake_account.data_len())))
        .expect("Should have at least the payed rent");

        withdraw_inactive_sol(&withdraw_opts, amount)?;

        excess_removed = (excess_removed + amount)?;
        stake_observed_total = (stake_observed_total + account_balance)?;
    }

    // Solana has no slashing at the time of writing, and only Solido can
    // withdraw from these accounts, so we should not observe a decrease in
    // balance.
    Validator::observe_balance(
        stake_observed_total,
        validator.effective_stake_balance,
        "Stake",
    )?;

    // Try to withdraw from unstake accounts.
    let mut unstake_removed = Lamports(0);
    let mut unstake_observed_total = Lamports(0);
    for (seed, unstake_account) in validator.unstake_seeds.into_iter().zip(unstake_accounts) {
        let (unstake_account_address, _bump_seed) = validator.find_stake_account_address(
            program_id,
            accounts.lido.key,
            seed,
            StakeType::Unstake,
        );

        let account_balance = check_address_and_get_balance(
            &unstake_account_address,
            unstake_account,
            seed,
            StakeType::Unstake,
        )?;

        let withdraw_opts = WithdrawExcessOpts {
            accounts: &accounts,
            clock: &clock,
            stake_history: &stake_history,
            stake_account: unstake_account,
            stake_account_seed: seed,
            stake_authority_bump_seed: lido.stake_authority_bump_seed,
        };
        let stake_account = get_stake_account(&withdraw_opts)?;

        // If validator's stake is at the beginning, try to withdraw the full
        // amount, which leaves the account empty, so we also bump the begin
        // seed. Older accounts are at the start of the list, so it shouldn't
        // happen that unstake account i is not fully inactive, but account
        // j > i is. But in case it does happen, we should not leave holes in
        // the list of stake accounts, so we only withdraw from the beginning.
        // If an unstake account is still partially active, then for simplicity,
        // we don't withdraw anything; we can withdraw in a later epoch when the
        // account is 100% inactive. This means we would miss out on some
        // rewards, but in practice deactivation happens in a single epoch, and
        // this is not a concern.
        if validator.unstake_seeds.begin == seed
            && stake_account.balance.inactive == stake_account.balance.total()
        {
            withdraw_inactive_sol(&withdraw_opts, stake_account.balance.inactive)?;
            validator.unstake_seeds.begin += 1;
            unstake_removed = (unstake_removed + stake_account.balance.inactive)?;
        }
        unstake_observed_total = (unstake_observed_total + account_balance)?;
    }

    Validator::observe_balance(
        unstake_observed_total,
        validator.unstake_accounts_balance,
        "Unstake",
    )?;

    // we track stake_accounts_balance, so only rewards and
    // donations (which we consider rewards) can make a difference
    let stake_total_with_rewards = (stake_observed_total + unstake_observed_total)?;
    let rewards = (stake_total_with_rewards - validator.stake_accounts_balance)
        .expect("Does not underflow, because tracked balance <= total.");
    msg!("received rewards and donations {}.", rewards);

    // Store the new total. If we withdrew any inactive stake back to the
    // reserve, that is now no longer part of the stake accounts, so subtract
    // that + the total unstake removed.
    validator.unstake_accounts_balance = (unstake_observed_total - unstake_removed)
        .expect("Does not underflow, because excess <= total.");

    validator.stake_accounts_balance = stake_observed_total
        .sub(excess_removed)
        .expect("Does not underflow, because excess <= total.")
        .add(validator.unstake_accounts_balance)
        .expect("If Solido has enough SOL to make this overflow, something has gone very wrong.");

    validator.effective_stake_balance = validator.compute_effective_stake_balance();

    distribute_fees(&mut lido, &accounts, &clock, rewards)?;

    lido.save(accounts.lido)
}

/// Splits a stake account from a validator's stake account.
/// This function can only be called after the exchange rate is updated with
/// `process_update_exchange_rate`.
pub fn process_withdraw(
    program_id: &Pubkey,
    amount: StLamports,
    validator_index: u32,
    raw_accounts: &[AccountInfo],
) -> ProgramResult {
    let accounts = WithdrawAccountsInfoV2::try_from_slice(raw_accounts)?;
    let mut lido = Lido::deserialize_lido(program_id, accounts.lido)?;
    let clock = Clock::get()?;
    lido.check_exchange_rate_last_epoch(&clock, "Withdraw")?;

    let validator_list_data = &mut *accounts.validator_list.data.borrow_mut();
    let mut validators = lido.deserialize_account_list_info::<Validator>(
        program_id,
        accounts.validator_list,
        validator_list_data,
    )?;

    // Confirm that there is no other validator with a higher balance that
    // we could withdraw from. This alone is not sufficient to guarantee a uniform
    // stake balance, but prevents things from becoming more unbalanced than
    // necessary.
    let maximum_stake_validator = validators
        .iter()
        .max_by_key(|pair| pair.effective_stake_balance)
        .ok_or(LidoError::NoActiveValidators)?;
    let maximum_stake_pubkey = *maximum_stake_validator.pubkey();
    let maximum_stake_balance = maximum_stake_validator.effective_stake_balance;

    // We should withdraw from the validator that has the most effective stake.
    // With effective here we mean "total in stake accounts" - "total in unstake
    // accounts", regardless of whether the stake in those accounts is active or not.
    let validator = validators.get_mut(validator_index, accounts.validator_vote_account.key)?;

    if validator.effective_stake_balance == Lamports(0) {
        msg!("Validator {} has no stake", validator.pubkey());
        return Err(LidoError::ValidatorHasNoStake.into());
    }

    // Note that we compare balances, not keys, because the maximum might not be unique.
    if validator.effective_stake_balance < maximum_stake_balance {
        msg!(
            "Refusing to withdraw from {}, who has {} stake, \
            because {} has more stake: {}. Withdraw from there instead.",
            validator.pubkey(),
            validator.effective_stake_balance,
            maximum_stake_pubkey,
            maximum_stake_balance,
        );
        return Err(LidoError::ValidatorWithMoreStakeExists.into());
    }

    let (stake_account, _) = validator.find_stake_account_address(
        program_id,
        accounts.lido.key,
        validator.stake_seeds.begin,
        StakeType::Stake,
    );
    if &stake_account != accounts.source_stake_account.key {
        msg!("Stake account is different than the calculated by the given seed, should be {}, is {}.",
        stake_account, accounts.source_stake_account.key);
        return Err(LidoError::InvalidStakeAccount.into());
    }

    // Reduce validator's balance
    let sol_to_withdraw = match lido.exchange_rate.exchange_st_sol(amount) {
        Ok(amount) => amount,
        Err(err) => {
            msg!("Cannot exchange stSOL for SOL, because no stSTOL has been minted.");
            return Err(err.into());
        }
    };

    let source_balance = Lamports(accounts.source_stake_account.lamports());

    // Limit the amount to withdraw to 10% of the stake account's balance + a
    // small constant. The 10% caps the imbalance that a withdrawal can create
    // at large balances, and in that case the constant is negligible, but the
    // constant does ensure that we can reach the minimum in a finite number of
    // withdrawals.
    let max_withdraw_amount = (source_balance
        * Rational {
            numerator: 1,
            denominator: 10,
        })
    .expect("Multiplying with 0.1 does not overflow or divide by zero.")
    .add(Lamports(10 * LAMPORTS_PER_SOL))?;

    if sol_to_withdraw > max_withdraw_amount {
        msg!(
            "To keep the pool balanced, you can withdraw at most {} from this \
            validator, but you are trying to withdraw {}.",
            max_withdraw_amount,
            sol_to_withdraw,
        );
        msg!("Please break up your withdrawal into multiple smaller withdrawals.");
        return Err(LidoError::InvalidAmount.into());
    }

    let remaining_balance = (source_balance - sol_to_withdraw)?;
    if remaining_balance < MINIMUM_STAKE_ACCOUNT_BALANCE {
        msg!("Withdrawal will leave the stake account with less than the minimum stake account balance.
        Maximum amount to withdraw is {}, tried to withdraw {}",
        (Lamports(accounts.source_stake_account.lamports()) - MINIMUM_STAKE_ACCOUNT_BALANCE)
        .expect("We do not allow the balance to fall below the minimum"), sol_to_withdraw);
        return Err(LidoError::InvalidAmount.into());
    }

    validator.stake_accounts_balance = (validator.stake_accounts_balance - sol_to_withdraw)?;
    validator.effective_stake_balance = validator.compute_effective_stake_balance();

    // Burn stSol tokens
    burn_st_sol(&lido, &accounts, amount)?;

    // Update withdrawal metrics.
    lido.metrics.observe_withdrawal(amount, sol_to_withdraw)?;

    split_stake_account(
        accounts.lido.key,
        &lido,
        &SplitStakeAccounts {
            source_stake_account: accounts.source_stake_account,
            destination_stake_account: accounts.destination_stake_account,
            authority: accounts.stake_authority,
            system_program: accounts.system_program,
            stake_program: accounts.stake_program,
        },
        sol_to_withdraw,
        &[],
    )?;

    // Give control of the stake to the user.
    transfer_stake_authority(&accounts, lido.stake_authority_bump_seed)?;

    // Explain what we did in the logs, because block explorers can be an
    // inscrutable mess of accounts, especially without special parsers for
    // Solido transactions. With the logs, we can still identify what happened.
    msg!("Solido: Withdrew {} for {}.", amount, sol_to_withdraw);

    lido.save(accounts.lido)
}

/// Migrate Solido state to version 2
pub fn processor_migrate_to_v2(
    program_id: &Pubkey,
    reward_distribution: RewardDistribution,
    max_validators: u32,
    max_maintainers: u32,
    max_commission_percentage: u8,
    accounts_raw: &[AccountInfo],
) -> ProgramResult {
    let accounts = MigrateStateToV2Info::try_from_slice(accounts_raw)?;
    let lido_v1 = LidoV1::deserialize_lido(program_id, accounts.lido)?;
    lido_v1.check_manager(accounts.manager)?;

    if !(lido_v1.lido_version == 0 && Lido::VERSION == 1) {
        return Err(LidoError::LidoVersionMismatch.into());
    }

    let rent = &Rent::get()?;
    check_rent_exempt(rent, accounts.validator_list, "Validator list account")?;
    check_rent_exempt(rent, accounts.maintainer_list, "Maintainer list account")?;

    check_account_owner(accounts.validator_list, program_id)?;
    check_account_owner(accounts.maintainer_list, program_id)?;

    check_account_data(
        accounts.validator_list,
        ValidatorList::required_bytes(max_validators),
        AccountType::Validator,
    )?;
    check_account_data(
        accounts.maintainer_list,
        MaintainerList::required_bytes(max_maintainers),
        AccountType::Maintainer,
    )?;

    if accounts.validator_list.key == accounts.maintainer_list.key {
        msg!("Cannot use same account for validator list and maintainer list");
        return Err(LidoError::AlreadyInUse.into());
    }

    if lido_v1.maintainers.entries.len() > max_maintainers as usize {
        msg!("max_maintainers is too small");
        return Err(LidoError::MaximumNumberOfAccountsExceeded.into());
    }
    let mut maintainers = MaintainerList::new_default(0);
    maintainers.header.max_entries = max_maintainers;

    for pe in lido_v1.maintainers.entries {
        maintainers.entries.push(Maintainer::new(pe.pubkey));
    }

    if lido_v1.validators.entries.len() > max_validators as usize {
        msg!("max_validators is too small");
        return Err(LidoError::MaximumNumberOfAccountsExceeded.into());
    }
    let mut validators = ValidatorList::new_default(0);
    validators.header.max_entries = max_validators;

    if !lido_v1.validators.entries.is_empty() {
        msg!("There should be no validators in Solido state prior to update.");
        msg!("You should first deactivate all validators and wait for epoch boundary.");
        msg!("Then maintainers will withdraw inactive stake to reserve and remove validators.");
        return Err(LidoError::ValidatorListNotEmpty.into());
    }

    if max_commission_percentage > 100 {
        return Err(LidoError::ValidationCommissionOutOfBounds.into());
    }

    // Initialize Lido structure
    let lido = Lido {
        lido_version: Lido::VERSION,
        account_type: AccountType::Lido,
        validator_list: *accounts.validator_list.key,
        maintainer_list: *accounts.maintainer_list.key,
        max_commission_percentage,
        fee_recipients: FeeRecipients {
            treasury_account: lido_v1.fee_recipients.treasury_account,
            developer_account: *accounts.developer_account.key,
        },
        reward_distribution,

        manager: lido_v1.manager,
        st_sol_mint: lido_v1.st_sol_mint,
        exchange_rate: lido_v1.exchange_rate,
        sol_reserve_account_bump_seed: lido_v1.sol_reserve_account_bump_seed,
        mint_authority_bump_seed: lido_v1.mint_authority_bump_seed,
        stake_authority_bump_seed: lido_v1.stake_authority_bump_seed,
        metrics: lido_v1.metrics,
    };

    // Confirm that the fee recipients are actually stSOL accounts.
    lido.check_is_st_sol_account(accounts.developer_account)?;

    validators.save(accounts.validator_list)?;
    maintainers.save(accounts.maintainer_list)?;
    lido.save(accounts.lido)
}

/// Processes [Instruction](enum.Instruction.html).
pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], input: &[u8]) -> ProgramResult {
    let instruction = LidoInstruction::try_from_slice(input)?;
    match instruction {
        LidoInstruction::Initialize {
            reward_distribution,
            max_validators,
            max_maintainers,
            max_commission_percentage,
        } => process_initialize(
            program_id,
            reward_distribution,
            max_validators,
            max_maintainers,
            max_commission_percentage,
            accounts,
        ),
        LidoInstruction::Deposit { amount } => process_deposit(program_id, amount, accounts),
        LidoInstruction::StakeDepositV2 {
            amount,
            validator_index,
            maintainer_index,
        } => process_stake_deposit(
            program_id,
            amount,
            validator_index,
            maintainer_index,
            accounts,
        ),
        LidoInstruction::UnstakeV2 {
            amount,
            validator_index,
            maintainer_index,
        } => process_unstake(
            program_id,
            amount,
            validator_index,
            maintainer_index,
            accounts,
        ),
        LidoInstruction::UpdateExchangeRateV2 => process_update_exchange_rate(program_id, accounts),
        LidoInstruction::UpdateStakeAccountBalance { validator_index } => {
            process_update_stake_account_balance(program_id, validator_index, accounts)
        }
        LidoInstruction::WithdrawV2 {
            amount,
            validator_index,
        } => process_withdraw(program_id, amount, validator_index, accounts),
        LidoInstruction::ChangeRewardDistribution {
            new_reward_distribution,
        } => process_change_reward_distribution(program_id, new_reward_distribution, accounts),
        LidoInstruction::AddValidatorV2 => process_add_validator(program_id, accounts),
        LidoInstruction::RemoveValidatorV2 { validator_index } => {
            process_remove_validator(program_id, validator_index, accounts)
        }
        LidoInstruction::DeactivateValidatorV2 { validator_index } => {
            process_deactivate_validator(program_id, validator_index, accounts)
        }
        LidoInstruction::AddMaintainerV2 => process_add_maintainer(program_id, accounts),
        LidoInstruction::RemoveMaintainerV2 { maintainer_index } => {
            process_remove_maintainer(program_id, maintainer_index, accounts)
        }
        LidoInstruction::MergeStakeV2 { validator_index } => {
            process_merge_stake(program_id, validator_index, accounts)
        }
        LidoInstruction::DeactivateValidatorIfCommissionExceedsMax { validator_index } => {
            process_deactivate_validator_if_commission_exceeds_max(
                program_id,
                validator_index,
                accounts,
            )
        }
        LidoInstruction::SetMaxValidationCommission {
            max_commission_percentage,
        } => process_set_max_commission_percentage(program_id, max_commission_percentage, accounts),
        LidoInstruction::MigrateStateToV2 {
            reward_distribution,
            max_validators,
            max_maintainers,
            max_commission_percentage,
        } => processor_migrate_to_v2(
            program_id,
            reward_distribution,
            max_validators,
            max_maintainers,
            max_commission_percentage,
            accounts,
        ),
        LidoInstruction::WithdrawInactiveStake
        | LidoInstruction::CollectValidatorFee
        | LidoInstruction::ClaimValidatorFee
        | LidoInstruction::UpdateExchangeRate
        | LidoInstruction::MergeStake
        | LidoInstruction::AddValidator
        | LidoInstruction::AddMaintainer
        | LidoInstruction::DeactivateValidator
        | LidoInstruction::RemoveMaintainer
        | LidoInstruction::RemoveValidator
        | LidoInstruction::StakeDeposit { .. }
        | LidoInstruction::Unstake { .. }
        | LidoInstruction::Withdraw { .. } => {
            msg!("{:?} is no longer supported since v2. Please check the changelog in the repository for update instructions.", instruction);
            Err(LidoError::InstructionIsDeprecated.into())
        }
    }
}
