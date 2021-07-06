//! Program state processor

use spl_stake_pool::stake_program;

use crate::{
    error::LidoError,
    instruction::{
        DepositAccountsInfo, InitializeAccountsInfo, LidoInstruction, StakeDepositAccountsInfo,
        UpdateExchangeRateAccountsInfo, UpdateValidatorBalanceInfo,
    },
    logic::{
        check_rent_exempt, create_account_overwrite_if_exists, deserialize_lido, distribute_fees,
        initialize_stake_account_undelegated, mint_st_sol_to,
    },
    process_management::{
        process_add_maintainer, process_add_validator, process_change_reward_distribution,
        process_claim_validator_fee, process_merge_stake, process_remove_maintainer,
        process_remove_validator,
    },
    state::{
        FeeRecipients, Lido, Maintainers, RewardDistribution, Validator, Validators,
        LIDO_CONSTANT_SIZE, LIDO_VERSION,
    },
    token::{Lamports, StLamports},
    MINT_AUTHORITY, RESERVE_ACCOUNT, STAKE_AUTHORITY, VALIDATOR_STAKE_ACCOUNT,
};

use {
    borsh::BorshDeserialize,
    solana_program::{
        account_info::AccountInfo,
        clock::Clock,
        entrypoint::ProgramResult,
        msg,
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

    let mut lido = deserialize_lido(program_id, accounts.lido)?;
    lido.is_initialized()?;

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
    // Initialize fee structure
    lido.fee_recipients = FeeRecipients {
        treasury_account: *accounts.treasury_account.key,
        developer_account: *accounts.developer_account.key,
    };

    let (_, reserve_bump_seed) = Pubkey::find_program_address(
        &[&accounts.lido.key.to_bytes(), RESERVE_ACCOUNT],
        program_id,
    );

    let (_, deposit_bump_seed) = Pubkey::find_program_address(
        &[&accounts.lido.key.to_bytes(), STAKE_AUTHORITY],
        program_id,
    );

    let (_, mint_bump_seed) =
        Pubkey::find_program_address(&[&accounts.lido.key.to_bytes(), MINT_AUTHORITY], program_id);

    lido.lido_version = version;
    lido.maintainers = Maintainers::new(max_maintainers);
    lido.validators = Validators::new(max_validators);
    lido.manager = *accounts.manager.key;
    lido.st_sol_mint = *accounts.st_sol_mint.key;
    lido.sol_reserve_account_bump_seed = reserve_bump_seed;
    lido.mint_authority_bump_seed = mint_bump_seed;
    lido.stake_authority_bump_seed = deposit_bump_seed;
    lido.reward_distribution = reward_distribution;

    // Confirm that the fee recipients are actually stSOL accounts.
    lido.check_is_st_sol_account(&accounts.treasury_account)?;
    lido.check_is_st_sol_account(&accounts.developer_account)?;

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

    let lido = deserialize_lido(program_id, accounts.lido)?;

    invoke(
        &system_instruction::transfer(accounts.user.key, accounts.reserve_account.key, amount.0),
        &[
            accounts.user.clone(),
            accounts.reserve_account.clone(),
            accounts.system_program.clone(),
        ],
    )?;

    let st_sol_amount = lido
        .exchange_rate
        .exchange_sol(amount)
        .ok_or(LidoError::CalculationFailure)?;

    mint_st_sol_to(
        &lido,
        accounts.lido.key,
        accounts.spl_token,
        accounts.st_sol_mint,
        accounts.mint_authority,
        accounts.recipient,
        st_sol_amount,
    )?;

    Ok(())
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
        .get_mut(&accounts.validator_vote_account.key)?;

    let stake_account_bump_seed = Lido::check_stake_account(
        program_id,
        &accounts.lido.key,
        &validator,
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
    let account_data_size = std::mem::size_of::<stake_program::StakeState>();
    let account_owner = stake_program::id();
    create_account_overwrite_if_exists(
        &accounts.lido.key,
        amount,
        account_data_size as u64,
        &account_owner,
        accounts.stake_account_end,
        accounts.reserve,
        lido.sol_reserve_account_bump_seed,
        accounts.system_program,
        stake_account_seeds,
    )?;

    // Now initialize the stake, but do not yet delegate it.
    initialize_stake_account_undelegated(
        accounts.stake_authority.key,
        accounts.stake_account_end,
        accounts.sysvar_rent,
        accounts.stake_program,
    )?;

    // Read the new balance. If there was balance in the stake account already,
    // then the amount we actually put in the stake account might be higher than
    // the amount transferred from the reserve.
    let amount_staked = Lamports(accounts.stake_account_end.lamports());
    msg!("Created stake account that holds {}.", amount_staked);

    // Update the amount staked for this validator.
    validator.entry.stake_accounts_balance = (validator.entry.stake_accounts_balance
        + amount_staked)
        .ok_or(LidoError::CalculationFailure)?;

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
            &stake_program::delegate_stake(
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
            &accounts.lido.key,
            &validator,
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
        invoke_signed(
            &stake_program::merge(
                accounts.stake_account_merge_into.key,
                accounts.stake_account_end.key,
                accounts.stake_authority.key,
            ),
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

pub fn process_update_validator_balance(
    program_id: &Pubkey,
    raw_accounts: &[AccountInfo],
) -> ProgramResult {
    let accounts = UpdateValidatorBalanceInfo::try_from_slice(raw_accounts)?;
    let mut lido = deserialize_lido(program_id, accounts.lido)?;

    // Confirm that the passed accounts are the ones configured in the state,
    // and confirm that they can receive stSOL.
    lido.check_mint_is_st_sol_mint(accounts.st_sol_mint)?;
    lido.check_treasury_fee_st_sol_account(accounts.treasury_st_sol_account)?;
    lido.check_developer_fee_st_sol_account(accounts.developer_st_sol_account)?;

    let clock = Clock::from_account_info(accounts.sysvar_clock)?;
    if lido.exchange_rate.computed_in_epoch < clock.epoch {
        msg!(
            "The exchange rate is outdated, it was last computed in epoch {}, \
            but now it is epoch {}.",
            lido.exchange_rate.computed_in_epoch,
            clock.epoch,
        );
        msg!("Please call UpdateExchangeRate before calling UpdateValidatorBalance.");
        return Err(LidoError::ExchangeRateNotUpdatedInThisEpoch.into());
    }

    let validator = lido
        .validators
        .get_mut(accounts.validator_vote_account.key)?;

    let mut observed_total = Lamports(0);
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

        observed_total = (observed_total + account_balance).ok_or(LidoError::CalculationFailure)?;
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
    // the excess is the validation reward paid into the account. (Or a donation
    // by some joker, we treat those the same way.)
    let rewards = (observed_total - validator.entry.stake_accounts_balance)
        .expect("Does not underflow because observed_total >= stake_accounts_balance.");
    msg!("{} in rewards observed.", rewards);

    // Store the new total, so we only distribute these rewards once.
    validator.entry.stake_accounts_balance = observed_total;

    let fees = lido
        .reward_distribution
        .split_reward(rewards, lido.validators.len() as u64)
        .ok_or(LidoError::CalculationFailure)?;
    distribute_fees(&mut lido, &accounts, fees)?;

    lido.save(accounts.lido)
}

// TODO(#93) Implement withdraw
pub fn process_withdraw(
    _program_id: &Pubkey,
    _pool_tokens: StLamports,
    _accounts: &[AccountInfo],
) -> ProgramResult {
    Ok(())
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
        LidoInstruction::UpdateValidatorBalance => {
            process_update_validator_balance(program_id, accounts)
        }
        LidoInstruction::Withdraw { amount } => process_withdraw(program_id, amount, accounts),
        LidoInstruction::ClaimValidatorFees => process_claim_validator_fee(program_id, accounts),
        LidoInstruction::ChangeRewardDistribution {
            new_reward_distribution,
        } => process_change_reward_distribution(program_id, new_reward_distribution, accounts),
        LidoInstruction::AddValidator { weight } => {
            process_add_validator(program_id, weight, accounts)
        }
        LidoInstruction::RemoveValidator => process_remove_validator(program_id, accounts),
        LidoInstruction::AddMaintainer => process_add_maintainer(program_id, accounts),
        LidoInstruction::RemoveMaintainer => process_remove_maintainer(program_id, accounts),
        LidoInstruction::MergeStake => process_merge_stake(program_id, accounts),
    }
}
