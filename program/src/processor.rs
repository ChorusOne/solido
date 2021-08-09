// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

//! Program state processor

use crate::{
    error::LidoError,
    instruction::{
        CollectValidatorFeeInfo, DepositAccountsInfo, InitializeAccountsInfo, LidoInstruction,
        StakeDepositAccountsInfo, UpdateExchangeRateAccountsInfo, WithdrawAccountsInfo,
        WithdrawInactiveStakeInfo,
    },
    logic::{
        burn_st_sol, check_mint, check_rent_exempt, create_account_overwrite_if_exists,
        deserialize_lido, distribute_fees, initialize_stake_account_undelegated, mint_st_sol_to,
        transfer_stake_authority, CreateAccountOptions,
    },
    metrics::Metrics,
    process_management::{
        process_add_maintainer, process_add_validator, process_change_reward_distribution,
        process_claim_validator_fee, process_merge_stake, process_remove_maintainer,
        process_remove_validator,
    },
    stake_account::{deserialize_stake_account, StakeAccount},
    state::{
        ExchangeRate, FeeRecipients, Lido, Maintainers, RewardDistribution, Validator, Validators,
        LIDO_CONSTANT_SIZE, LIDO_VERSION,
    },
    token::{Lamports, Rational, StLamports},
    vote_instruction, MINIMUM_STAKE_ACCOUNT_BALANCE, MINT_AUTHORITY, RESERVE_ACCOUNT,
    REWARDS_WITHDRAW_AUTHORITY, STAKE_AUTHORITY, VALIDATOR_STAKE_ACCOUNT,
};

use solana_program::stake::{self as stake_program};
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
    version: u8,
    program_id: &Pubkey,
    reward_distribution: RewardDistribution,
    max_validators: u32,
    max_maintainers: u32,
    accounts_raw: &[AccountInfo],
) -> ProgramResult {
    let accounts = InitializeAccountsInfo::try_from_slice(accounts_raw)?;
    let rent = &Rent::from_account_info(accounts.sysvar_rent)?;
    check_rent_exempt(rent, accounts.lido, "Solido account")?;
    check_rent_exempt(rent, accounts.reserve_account, "Reserve account")?;

    let is_uninitialized = accounts.lido.data.borrow()[..LIDO_CONSTANT_SIZE]
        .iter()
        .all(|byte| *byte == 0);
    if !is_uninitialized {
        msg!(
            "Account {} appears to be in use already, refusing to overwrite.",
            accounts.lido.key
        );
        return Err(LidoError::AlreadyInUse.into());
    }

    // Bytes required for maintainers
    let bytes_for_maintainers = Maintainers::required_bytes(max_maintainers as usize);
    // Bytes required for validators
    let bytes_for_validators = Validators::required_bytes(max_validators as usize);
    // Calculate the expected lido's size
    let bytes_sum = LIDO_CONSTANT_SIZE + bytes_for_validators + bytes_for_maintainers;
    if bytes_sum != accounts.lido.data_len() {
        msg!("Incorrect allocated bytes for the provided constrains: max_validator bytes: {}, max_maintainers bytes: {}, constant_size: {}, sum is {}, should be {}", bytes_for_validators, bytes_for_maintainers, LIDO_CONSTANT_SIZE, bytes_sum, accounts.lido.data_len());
        return Err(LidoError::InvalidLidoSize.into());
    }

    let (_, reserve_bump_seed) = Pubkey::find_program_address(
        &[&accounts.lido.key.to_bytes(), RESERVE_ACCOUNT],
        program_id,
    );

    let (_, deposit_bump_seed) = Pubkey::find_program_address(
        &[&accounts.lido.key.to_bytes(), STAKE_AUTHORITY],
        program_id,
    );

    let (mint_authority, mint_bump_seed) =
        Pubkey::find_program_address(&[&accounts.lido.key.to_bytes(), MINT_AUTHORITY], program_id);
    // Check if the token has no minted tokens and right mint authority.
    check_mint(rent, accounts.st_sol_mint, &mint_authority)?;

    let (_, rewards_withdraw_authority_bump_seed) = Pubkey::find_program_address(
        &[&accounts.lido.key.to_bytes(), REWARDS_WITHDRAW_AUTHORITY],
        program_id,
    );

    // Initialize fee structure
    let lido = Lido {
        lido_version: version,
        manager: *accounts.manager.key,
        st_sol_mint: *accounts.st_sol_mint.key,
        exchange_rate: ExchangeRate::default(),
        sol_reserve_account_bump_seed: reserve_bump_seed,
        mint_authority_bump_seed: mint_bump_seed,
        stake_authority_bump_seed: deposit_bump_seed,
        rewards_withdraw_authority_bump_seed,
        reward_distribution,
        fee_recipients: FeeRecipients {
            treasury_account: *accounts.treasury_account.key,
            developer_account: *accounts.developer_account.key,
        },
        metrics: Metrics::new(),
        maintainers: Maintainers::new(max_maintainers),
        validators: Validators::new(max_validators),
    };

    // Confirm that the fee recipients are actually stSOL accounts.
    lido.check_is_st_sol_account(accounts.treasury_account)?;
    lido.check_is_st_sol_account(accounts.developer_account)?;

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

    let mut lido = deserialize_lido(program_id, accounts.lido)?;

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

    lido.metrics.deposit_amount.observe(amount)?;
    lido.save(accounts.lido)
}

pub fn process_stake_deposit(
    program_id: &Pubkey,
    amount: Lamports,
    raw_accounts: &[AccountInfo],
) -> ProgramResult {
    let accounts = StakeDepositAccountsInfo::try_from_slice(raw_accounts)?;

    let mut lido = deserialize_lido(program_id, accounts.lido)?;

    lido.check_maintainer(accounts.maintainer)?;
    lido.check_reserve_account(program_id, accounts.lido.key, accounts.reserve)?;
    lido.check_stake_authority(program_id, accounts.lido.key, accounts.stake_authority)?;
    lido.check_can_stake_amount(accounts.reserve, accounts.sysvar_rent, amount)?;

    let validator = lido
        .validators
        .get_mut(accounts.validator_vote_account.key)?;
    if validator.entry.inactive {
        msg!(
            "Validator {} is inactive, new deposits are not allowed",
            validator.pubkey
        );
        return Err(LidoError::StakeToInactiveValidator.into());
    }

    let stake_account_bump_seed = Lido::check_stake_account(
        program_id,
        accounts.lido.key,
        validator,
        validator.entry.stake_accounts_seed_end,
        accounts.stake_account_end,
    )?;

    if accounts.stake_account_end.data.borrow().len() > 0 {
        msg!(
            "Stake account {} contains data, aborting.",
            accounts.stake_account_end.key
        );
        return Err(LidoError::WrongStakeState.into());
    }

    let stake_account_seed = validator.entry.stake_accounts_seed_end.to_le_bytes();
    let stake_account_bump_seed = [stake_account_bump_seed];
    let stake_account_seeds = &[
        accounts.lido.key.as_ref(),
        validator.pubkey.as_ref(),
        VALIDATOR_STAKE_ACCOUNT,
        &stake_account_seed[..],
        &stake_account_bump_seed[..],
    ][..];

    // Create the account that is going to hold the new stake account data.
    // Even if it was already funded.
    create_account_overwrite_if_exists(
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
    validator.entry.stake_accounts_balance = (validator.entry.stake_accounts_balance + amount)?;

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
    if accounts.stake_account_end.key == accounts.stake_account_merge_into.key {
        // Case 1: we delegate, and we don't touch `stake_account_merge_into`.
        msg!(
            "Delegating stake account at seed {} ...",
            validator.entry.stake_accounts_seed_end
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
        validator.entry.stake_accounts_seed_end += 1;
    } else {
        // Case 2: Merge the new undelegated stake account into the existing one.
        if validator.entry.stake_accounts_seed_end <= validator.entry.stake_accounts_seed_begin {
            msg!("Can only stake-merge if there is at least one stake account to merge into.");
            return Err(LidoError::InvalidStakeAccount.into());
        }
        Lido::check_stake_account(
            program_id,
            accounts.lido.key,
            validator,
            // Does not underflow, because end > begin >= 0.
            validator.entry.stake_accounts_seed_end - 1,
            accounts.stake_account_merge_into,
        )?;
        // The stake program checks that the two accounts can be merged; if we
        // tried to merge, but the epoch is different, then this will fail.
        msg!(
            "Merging into existing stake account at seed {} ...",
            validator.entry.stake_accounts_seed_end - 1
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

    lido.save(accounts.lido)
}

pub fn process_update_exchange_rate(
    program_id: &Pubkey,
    raw_accounts: &[AccountInfo],
) -> ProgramResult {
    let accounts = UpdateExchangeRateAccountsInfo::try_from_slice(raw_accounts)?;
    let mut lido = deserialize_lido(program_id, accounts.lido)?;
    lido.check_reserve_account(program_id, accounts.lido.key, accounts.reserve)?;

    let clock = Clock::from_account_info(accounts.sysvar_clock)?;
    let rent = Rent::from_account_info(accounts.sysvar_rent)?;

    if lido.exchange_rate.computed_in_epoch >= clock.epoch {
        msg!(
            "The exchange rate was already updated in epoch {}.",
            lido.exchange_rate.computed_in_epoch
        );
        msg!("It can only be done once per epoch, so we are going to abort this transaction.");
        return Err(LidoError::ExchangeRateAlreadyUpToDate.into());
    }

    lido.exchange_rate.computed_in_epoch = clock.epoch;
    lido.exchange_rate.sol_balance = lido.get_sol_balance(&rent, accounts.reserve)?;
    lido.exchange_rate.st_sol_supply = lido.get_st_sol_supply(accounts.st_sol_mint)?;

    lido.save(accounts.lido)
}

/// If a stake account contains more inactive SOL than needed to make it rent-exempt,
/// withdraw the excess back to the reserve. Returns how much SOL was withdrawn.
pub fn withdraw_excess_inactive_sol<'a, 'b>(
    accounts: &WithdrawInactiveStakeInfo<'a, 'b>,
    clock: &Clock,
    rent: &Rent,
    stake_history: &StakeHistory,
    stake_account: &AccountInfo<'b>,
    stake_account_seed: u64,
    stake_authority_bump_seed: u8,
) -> Result<Lamports, ProgramError> {
    let stake_account_rent = Lamports(rent.minimum_balance(stake_account.data_len()));
    let stake = deserialize_stake_account(&stake_account.data.borrow())?;
    let stake_info = StakeAccount::from_delegated_account(
        Lamports(stake_account.lamports()),
        &stake,
        clock,
        stake_history,
        stake_account_seed,
    );
    let excess_balance = Lamports(
        stake_info
            .balance
            .inactive
            .0
            .saturating_sub(stake_account_rent.0),
    );
    if excess_balance > Lamports(0) {
        let withdraw_instruction = StakeAccount::stake_account_withdraw(
            excess_balance,
            stake_account.key,
            accounts.reserve.key,
            accounts.stake_authority.key,
        );
        invoke_signed(
            &withdraw_instruction,
            &[
                stake_account.clone(),
                accounts.reserve.clone(),
                accounts.sysvar_clock.clone(),
                accounts.sysvar_stake_history.clone(),
                accounts.stake_authority.clone(),
                accounts.stake_program.clone(),
            ],
            &[&[
                accounts.lido.key.as_ref(),
                STAKE_AUTHORITY,
                &[stake_authority_bump_seed],
            ]],
        )?;
        msg!(
            "Withdrew {} inactive stake back to the reserve from stake account at seed {}.",
            excess_balance,
            stake_account_seed
        );
    }

    Ok(excess_balance)
}

/// Recover any inactive balance from the validator's stake accounts to the
/// reserve account.
/// Updates the validator's balance.
/// This function is permissionless and can be called by anyone.
pub fn process_withdraw_inactive_stake(
    program_id: &Pubkey,
    raw_accounts: &[AccountInfo],
) -> ProgramResult {
    let accounts = WithdrawInactiveStakeInfo::try_from_slice(raw_accounts)?;
    let mut lido = deserialize_lido(program_id, accounts.lido)?;
    let rent = Rent::from_account_info(accounts.sysvar_rent)?;
    let stake_history = StakeHistory::from_account_info(accounts.sysvar_stake_history)?;
    let clock = Clock::from_account_info(accounts.sysvar_clock)?;

    // Confirm that the passed accounts are the ones configured in the state,
    // and confirm that they can receive stSOL.
    lido.check_reserve_account(program_id, accounts.lido.key, accounts.reserve)?;

    let validator = lido
        .validators
        .get_mut(accounts.validator_vote_account.key)?;

    let mut observed_total = Lamports(0);
    let mut excess_removed = Lamports(0);
    let mut stake_accounts = accounts.stake_accounts.iter();
    let begin = validator.entry.stake_accounts_seed_begin;
    let end = validator.entry.stake_accounts_seed_end;

    // Visit the stake accounts one by one, and check how much SOL is in there.
    for seed in begin..end {
        let (stake_account_address, _bump_seed) = Validator::find_stake_account_address(
            program_id,
            accounts.lido.key,
            &validator.pubkey,
            seed,
        );
        let stake_account = match stake_accounts.next() {
            None => {
                msg!(
                    "Not enough stake accounts provided, got {} but expected {}.",
                    accounts.stake_accounts.len(),
                    end - begin,
                );
                msg!("Account at seed {} is missing.", seed);
                return Err(LidoError::InvalidStakeAccount.into());
            }
            Some(account) if account.key != &stake_account_address => {
                msg!(
                    "Wrong stake account provided for seed {}: expected {} but got {}.",
                    seed,
                    stake_account_address,
                    account.key,
                );
                return Err(LidoError::InvalidStakeAccount.into());
            }
            Some(account) => account,
        };
        let account_balance = Lamports(**stake_account.lamports.borrow());
        msg!(
            "Stake account at seed {} ({}) contains {}.",
            seed,
            stake_account.key,
            account_balance
        );

        let excess_removed_here = withdraw_excess_inactive_sol(
            &accounts,
            &clock,
            &rent,
            &stake_history,
            stake_account,
            seed,
            lido.stake_authority_bump_seed,
        )?;

        excess_removed = (excess_removed + excess_removed_here)?;
        observed_total = (observed_total + account_balance)?;
    }

    if observed_total < validator.entry.stake_accounts_balance {
        // Solana has no slashing at the time of writing, and only Solido can
        // withdraw from these accounts, so we should not observe a decrease in
        // balance.
        msg!(
            "Observed balance of {} is less than tracked balance of {}.",
            observed_total,
            validator.entry.stake_accounts_balance
        );
        msg!("This should not happen, aborting ...");
        return Err(LidoError::ValidatorBalanceDecreased.into());
    }

    // We tracked in `stake_accounts_balance` what we put in there ourselves, so
    // the excess is a donation by some joker.
    let donation = (observed_total - validator.entry.stake_accounts_balance)
        .expect("Does not underflow because observed_total >= stake_accounts_balance.");
    msg!("{} in donations observed.", donation);

    // Store the new total. If we withdrew any inactive stake back to the
    // reserve, that is now no longer part of the stake accounts, so subtract
    // that.
    validator.entry.stake_accounts_balance =
        (observed_total - excess_removed).expect("Does not underflow, because excess <= total.");
    lido.save(accounts.lido)
}

/// Collects the validator fee from the validator vote account and distributes
/// this fee across the specified participants. It transfers the collected
/// Lamports to the reserve account, where they can be re-staked.
/// This function can only be called after the exchange rate is updated with
/// `process_update_exchange_rate`.
/// This function is permissionless and can be called by anyone.
pub fn process_collect_validator_fee(
    program_id: &Pubkey,
    raw_accounts: &[AccountInfo],
) -> ProgramResult {
    let accounts = CollectValidatorFeeInfo::try_from_slice(raw_accounts)?;
    let mut lido = deserialize_lido(program_id, accounts.lido)?;
    let rent = Rent::from_account_info(accounts.sysvar_rent)?;

    // Confirm that the passed accounts are the ones configured in the state,
    // and confirm that they can receive stSOL.
    lido.check_mint_is_st_sol_mint(accounts.st_sol_mint)?;
    lido.check_treasury_fee_st_sol_account(accounts.treasury_st_sol_account)?;
    lido.check_developer_fee_st_sol_account(accounts.developer_st_sol_account)?;
    lido.check_reserve_account(program_id, accounts.lido.key, accounts.reserve)?;

    let clock = Clock::from_account_info(accounts.sysvar_clock)?;
    lido.check_exchange_rate_last_epoch(&clock, "CollectValidatorFee")?;

    let rewards_withdraw_authority = lido.check_rewards_withdraw_authority(
        program_id,
        accounts.lido.key,
        accounts.rewards_withdraw_authority,
    )?;

    let vote_account_rent = rent.minimum_balance(accounts.validator_vote_account.data_len());
    // Subtract the rent from the vote account, we should remove those only once
    // validators are removed.
    let rewards = accounts
        .validator_vote_account
        .lamports()
        .checked_sub(vote_account_rent)
        .expect("Vote account should be rent exempt");

    let fees = lido
        .reward_distribution
        .split_reward(Lamports(rewards), lido.validators.len() as u64)?;
    distribute_fees(&mut lido, &accounts, fees)?;

    invoke_signed(
        &vote_instruction::withdraw(
            accounts.validator_vote_account.key,
            &rewards_withdraw_authority,
            rewards,
            accounts.reserve.key, // checked if is right before.
        ),
        &[
            accounts.validator_vote_account.clone(),
            accounts.reserve.clone(),
            accounts.rewards_withdraw_authority.clone(),
            accounts.vote_program.clone(),
        ],
        &[&[
            accounts.lido.key.as_ref(),
            REWARDS_WITHDRAW_AUTHORITY,
            &[lido.rewards_withdraw_authority_bump_seed],
        ]],
    )?;
    lido.save(accounts.lido)
}

/// Splits a stake account from a validator's stake account.
/// This function can only be called after the exchange rate is updated with
/// `process_update_exchange_rate`.
pub fn process_withdraw(
    program_id: &Pubkey,
    amount: StLamports,
    raw_accounts: &[AccountInfo],
) -> ProgramResult {
    use std::ops::Add;

    let accounts = WithdrawAccountsInfo::try_from_slice(raw_accounts)?;
    let mut lido = deserialize_lido(program_id, accounts.lido)?;
    let clock = Clock::from_account_info(accounts.sysvar_clock)?;
    lido.check_exchange_rate_last_epoch(&clock, "Withdraw")?;

    // Should withdraw from the validator that has most stake
    let provided_validator = lido.validators.get(accounts.validator_vote_account.key)?;

    for validator in lido.validators.entries.iter() {
        if validator.entry.stake_accounts_balance > provided_validator.entry.stake_accounts_balance
        {
            msg!(
                "Validator {} has more stake than validator {}",
                provided_validator.pubkey,
                validator.pubkey,
            );
            return Err(LidoError::ValidatorWithMoreStakeExists.into());
        }
    }
    let (stake_account, _) = Validator::find_stake_account_address(
        program_id,
        accounts.lido.key,
        accounts.validator_vote_account.key,
        provided_validator.entry.stake_accounts_seed_begin,
    );
    if &stake_account != accounts.source_stake_account.key {
        msg!("Stake account is different than the calculated by the given seed, should be {}, is {}.", stake_account, accounts.source_stake_account.key);
        return Err(LidoError::InvalidStakeAccount.into());
    }

    // Reduce validator's balance
    let sol_to_withdraw = lido.exchange_rate.exchange_st_sol(amount)?;
    let provided_validator = lido
        .validators
        .get_mut(accounts.validator_vote_account.key)?;

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

    provided_validator.entry.stake_accounts_balance =
        (provided_validator.entry.stake_accounts_balance - sol_to_withdraw)?;

    // Burn stSol tokens
    burn_st_sol(&lido, &accounts, amount)?;

    // Update withdrawal metrics.
    lido.metrics.observe_withdrawal(amount, sol_to_withdraw)?;

    // The Stake program already checks for a minimum rent on the destination
    // stake account inside the `split` function.

    // The Split instruction returns three instructions:
    //   0 - Allocate instruction.
    //   1 - Assign owner instruction.
    //   2 - Split stake instruction.
    let split_instructions = solana_program::stake::instruction::split(
        &stake_account,
        accounts.stake_authority.key,
        sol_to_withdraw.0,
        accounts.destination_stake_account.key,
    );
    assert_eq!(split_instructions.len(), 3);

    let (allocate_instruction, assign_instruction, split_instruction) = (
        &split_instructions[0],
        &split_instructions[1],
        &split_instructions[2],
    );

    invoke(
        allocate_instruction,
        &[
            accounts.destination_stake_account.clone(),
            accounts.system_program.clone(),
        ],
    )?;
    invoke(
        assign_instruction,
        &[
            accounts.destination_stake_account.clone(),
            accounts.system_program.clone(),
        ],
    )?;

    invoke_signed(
        split_instruction,
        &[
            accounts.source_stake_account.clone(),
            accounts.destination_stake_account.clone(),
            accounts.stake_authority.clone(),
            accounts.stake_program.clone(),
        ],
        &[&[
            &accounts.lido.key.to_bytes(),
            STAKE_AUTHORITY,
            &[lido.stake_authority_bump_seed],
        ]],
    )?;

    // Give control of the stake to the user.
    transfer_stake_authority(&accounts, lido.stake_authority_bump_seed)?;

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
        } => process_initialize(
            LIDO_VERSION,
            program_id,
            reward_distribution,
            max_validators,
            max_maintainers,
            accounts,
        ),
        LidoInstruction::Deposit { amount } => process_deposit(program_id, amount, accounts),
        LidoInstruction::StakeDeposit { amount } => {
            process_stake_deposit(program_id, amount, accounts)
        }
        LidoInstruction::UpdateExchangeRate => process_update_exchange_rate(program_id, accounts),
        LidoInstruction::WithdrawInactiveStake => {
            process_withdraw_inactive_stake(program_id, accounts)
        }
        LidoInstruction::CollectValidatorFee => process_collect_validator_fee(program_id, accounts),
        LidoInstruction::Withdraw { amount } => process_withdraw(program_id, amount, accounts),
        LidoInstruction::ClaimValidatorFee => process_claim_validator_fee(program_id, accounts),
        LidoInstruction::ChangeRewardDistribution {
            new_reward_distribution,
        } => process_change_reward_distribution(program_id, new_reward_distribution, accounts),
        LidoInstruction::AddValidator => process_add_validator(program_id, accounts),
        LidoInstruction::RemoveValidator => process_remove_validator(program_id, accounts),
        LidoInstruction::AddMaintainer => process_add_maintainer(program_id, accounts),
        LidoInstruction::RemoveMaintainer => process_remove_maintainer(program_id, accounts),
        LidoInstruction::MergeStake => process_merge_stake(program_id, accounts),
    }
}
