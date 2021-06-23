//! Program state processor

use spl_stake_pool::stake_program;

use crate::{
    error::LidoError,
    instruction::{
        DepositAccountsInfo, InitializeAccountsInfo, LidoInstruction, StakeDepositAccountsInfo,
    },
    logic::{
        calc_total_lamports, check_rent_exempt, deserialize_lido, get_reserve_available_amount,
        token_mint_to,
    },
    process_management::{
        process_add_maintainer, process_add_validator, process_change_fee_spec,
        process_claim_validator_fee, process_distribute_fees, process_remove_maintainer,
        process_remove_validator,
    },
    state::{
        FeeDistribution, FeeRecipients, Maintainers, Validator, Validators, LIDO_CONSTANT_SIZE,
        LIDO_VERSION,
    },
    token::{Lamports, StLamports},
    DEPOSIT_AUTHORITY, RESERVE_AUTHORITY, VALIDATOR_STAKE_ACCOUNT,
};

use {
    borsh::{BorshDeserialize, BorshSerialize},
    solana_program::{
        account_info::AccountInfo,
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
    fee_distribution: FeeDistribution,
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
        &[&accounts.lido.key.to_bytes(), RESERVE_AUTHORITY],
        program_id,
    );

    let (_, deposit_bump_seed) = Pubkey::find_program_address(
        &[&accounts.lido.key.to_bytes(), DEPOSIT_AUTHORITY],
        program_id,
    );

    lido.lido_version = version;
    lido.maintainers = Maintainers::new(max_maintainers);
    lido.validators = Validators::new(max_validators);
    lido.manager = *accounts.manager.key;
    lido.st_sol_mint = *accounts.st_sol_mint.key;
    lido.sol_reserve_authority_bump_seed = reserve_bump_seed;
    lido.deposit_authority_bump_seed = deposit_bump_seed;
    lido.fee_distribution = fee_distribution;

    // Confirm that the fee recipients are actually stSOL accounts.
    lido.check_is_st_sol_account(&accounts.treasury_account)?;
    lido.check_is_st_sol_account(&accounts.developer_account)?;

    // For some reason, calling lido.serialize(&mut *accounts.lido.data.borrow_mut())
    // stopped working; it leaves the data with size zero. As a workaround, write
    // it to an intermediate buffer instead, and copy the buffer to the account
    // data. ¯\_(ツ)_/¯
    let mut buf = Vec::new();
    lido.serialize(&mut buf)?;
    accounts.lido.data.borrow_mut()[..buf.len()].copy_from_slice(&buf[..]);
    Ok(())
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

    lido.check_is_st_sol_account(&accounts.recipient)?;
    lido.check_reserve_authority(program_id, accounts.lido.key, accounts.reserve_account)?;

    let rent = &Rent::from_account_info(accounts.sysvar_rent)?;

    let total_lamports = calc_total_lamports(&lido, accounts.reserve_account, rent)?;
    invoke(
        &system_instruction::transfer(accounts.user.key, accounts.reserve_account.key, amount.0),
        &[
            accounts.user.clone(),
            accounts.reserve_account.clone(),
            accounts.system_program.clone(),
        ],
    )?;

    let st_sol_amount = lido
        .calc_pool_tokens_for_deposit(amount, total_lamports)
        .ok_or(LidoError::CalculationFailure)?;

    token_mint_to(
        accounts.lido.key,
        accounts.spl_token.clone(),
        accounts.st_sol_mint.clone(),
        accounts.recipient.clone(),
        accounts.reserve_account.clone(),
        RESERVE_AUTHORITY,
        lido.sol_reserve_authority_bump_seed,
        st_sol_amount,
    )?;
    let total_st_sol =
        (lido.st_sol_total_shares + st_sol_amount).ok_or(LidoError::CalculationFailure)?;

    lido.st_sol_total_shares = total_st_sol;

    lido.serialize(&mut *accounts.lido.data.borrow_mut())
        .map_err(|e| e.into())
}

pub fn process_stake_deposit(
    program_id: &Pubkey,
    amount: Lamports,
    raw_accounts: &[AccountInfo],
) -> ProgramResult {
    let accounts = StakeDepositAccountsInfo::try_from_slice(raw_accounts)?;

    let rent = &Rent::from_account_info(accounts.sysvar_rent)?;
    let mut lido = deserialize_lido(program_id, accounts.lido)?;
    lido.check_maintainer(accounts.maintainer)?;

    let minimum_stake_balance =
        Lamports(rent.minimum_balance(std::mem::size_of::<stake_program::StakeState>()));
    if amount < minimum_stake_balance {
        msg!("Trying to stake less than the minimum balance of a stake account.");
        msg!(
            "Need as least {} but got {}.",
            minimum_stake_balance,
            amount
        );
        return Err(LidoError::InvalidAmount.into());
    }

    let available_reserve_amount = get_reserve_available_amount(accounts.reserve, rent)?;
    if amount > available_reserve_amount {
        msg!("The requested amount {} is greater than the available amount {}, considering rent-exemption", amount, available_reserve_amount);
        return Err(LidoError::AmountExceedsReserve.into());
    }

    let validator = lido
        .validators
        .get_mut(&accounts.validator_vote_account.key)?;

    // TODO(#174) Merge into preceding stake account if possible
    let (stake_addr, stake_addr_bump_seed) = Validator::find_stake_account_address(
        program_id,
        accounts.lido.key,
        &validator.pubkey,
        validator.entry.stake_accounts_seed_end,
    );
    if &stake_addr != accounts.stake_account_end.key {
        msg!(
            "The derived stake address for seed {} is {}, but the instruction received {} instead.",
            validator.entry.stake_accounts_seed_end,
            stake_addr,
            accounts.stake_account_end.key,
        );
        msg!("This can happen when two StakeDeposit instructions race.");
        return Err(LidoError::InvalidStakeAccount.into());
    }

    let solido_address_bytes = accounts.lido.key.to_bytes();
    let reserve_authority_seed: &[&[_]] = &[&solido_address_bytes, RESERVE_AUTHORITY][..];
    let (reserve_authority, _) = Pubkey::find_program_address(reserve_authority_seed, program_id);

    if accounts.reserve.key != &reserve_authority {
        return Err(LidoError::InvalidReserveAuthority.into());
    }

    let reserve_account_bump_seed = [lido.sol_reserve_authority_bump_seed];
    let stake_account_seed = validator.entry.stake_accounts_seed_end.to_le_bytes();
    let stake_account_bump_seed = [stake_addr_bump_seed];
    let validator_vote_account_bytes = accounts.validator_vote_account.key.to_bytes();

    let reserve_account_seeds = &[
        &solido_address_bytes,
        RESERVE_AUTHORITY,
        &reserve_account_bump_seed[..],
    ][..];
    let stake_account_seeds = &[
        &solido_address_bytes,
        &validator_vote_account_bytes,
        VALIDATOR_STAKE_ACCOUNT,
        &stake_account_seed[..],
        &stake_account_bump_seed[..],
    ][..];

    // Confirm that the stake account is uninitialized, before we touch it.
    if accounts.stake_account_end.data.borrow().len() > 0 {
        return Err(LidoError::WrongStakeState.into());
    }

    // If the account is already funded, then `create_account` will fail. Some
    // joker could deposit some small amount into the stake account, and then we
    // would be stuck. So instead of `create_account`, we can do what it would
    // do anyway, without the funds check: `allocate`, `assign`, and then
    // `transfer`.
    invoke_signed(
        &system_instruction::allocate(
            accounts.stake_account_end.key,
            std::mem::size_of::<stake_program::StakeState>() as u64,
        ),
        &[
            accounts.stake_account_end.clone(),
            accounts.system_program.clone(),
        ],
        &[&stake_account_seeds],
    )?;
    invoke_signed(
        &system_instruction::assign(accounts.stake_account_end.key, &stake_program::id()),
        &[
            accounts.stake_account_end.clone(),
            accounts.system_program.clone(),
        ],
        &[&stake_account_seeds],
    )?;
    invoke_signed(
        &system_instruction::transfer(
            accounts.reserve.key,
            accounts.stake_account_end.key,
            amount.0,
        ),
        &[
            accounts.reserve.clone(),
            accounts.stake_account_end.clone(),
            accounts.system_program.clone(),
        ],
        &[&reserve_account_seeds, &stake_account_seeds],
    )?;

    // Now initialize the stake, and delegate it.
    invoke(
        &stake_program::initialize(
            accounts.stake_account_end.key,
            &stake_program::Authorized {
                staker: *accounts.deposit_authority.key,
                withdrawer: *accounts.deposit_authority.key,
            },
            &stake_program::Lockup::default(),
        ),
        &[
            accounts.stake_account_end.clone(),
            accounts.sysvar_rent.clone(),
            accounts.stake_program.clone(),
        ],
    )?;
    invoke_signed(
        &stake_program::delegate_stake(
            accounts.stake_account_end.key,
            accounts.deposit_authority.key,
            accounts.validator_vote_account.key,
        ),
        &[
            accounts.stake_account_end.clone(),
            accounts.validator_vote_account.clone(),
            accounts.sysvar_clock.clone(),
            accounts.stake_history.clone(),
            accounts.stake_program_config.clone(),
            accounts.deposit_authority.clone(),
            accounts.stake_program.clone(),
        ],
        &[&[
            &accounts.lido.key.to_bytes(),
            DEPOSIT_AUTHORITY,
            &[lido.deposit_authority_bump_seed],
        ]],
    )?;

    // Read the new balance. If there was balance in the stake account
    // already, then the amount we actually staked might be higher than the
    // amount transferred from the reserve.
    let amount_staked = Lamports(accounts.stake_account_end.lamports());

    // Update the total SOL that is activating for this validator.
    validator.entry.stake_accounts_balance = (validator.entry.stake_accounts_balance
        + amount_staked)
        .ok_or(LidoError::CalculationFailure)?;

    // We now consumed this stake account, bump the index.
    validator.entry.stake_accounts_seed_end += 1;

    lido.serialize(&mut *accounts.lido.data.borrow_mut())?;

    Ok(())
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
            fee_distribution,
            max_validators,
            max_maintainers,
        } => process_initialize(
            LIDO_VERSION,
            program_id,
            fee_distribution,
            max_validators,
            max_maintainers,
            accounts,
        ),
        LidoInstruction::Deposit { amount } => process_deposit(program_id, amount, accounts),
        LidoInstruction::StakeDeposit { amount } => {
            process_stake_deposit(program_id, amount, accounts)
        }
        LidoInstruction::Withdraw { amount } => process_withdraw(program_id, amount, accounts),
        LidoInstruction::DistributeFees => process_distribute_fees(program_id, accounts),
        LidoInstruction::ClaimValidatorFees => process_claim_validator_fee(program_id, accounts),
        LidoInstruction::ChangeFeeSpec {
            new_fee_distribution,
        } => process_change_fee_spec(program_id, new_fee_distribution, accounts),
        LidoInstruction::AddValidator => process_add_validator(program_id, accounts),
        LidoInstruction::RemoveValidator => process_remove_validator(program_id, accounts),
        LidoInstruction::AddMaintainer => process_add_maintainer(program_id, accounts),
        LidoInstruction::RemoveMaintainer => process_remove_maintainer(program_id, accounts),
    }
}
