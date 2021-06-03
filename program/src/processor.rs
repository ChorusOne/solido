//! Program state processor

use solana_program::program_pack::Pack;
use spl_stake_pool::{stake_program, state::StakePool};

use crate::{
    error::LidoError,
    instruction::{
        stake_pool_deposit, DepositAccountsInfo, DepositActiveStakeToPoolAccountsInfo,
        InitializeAccountsInfo, LidoInstruction, StakeDepositAccountsInfo,
        StakePoolDepositAccountsMeta,
    },
    logic::{
        calc_total_lamports, check_reserve_authority, deserialize_lido,
        get_reserve_available_amount, rent_exemption, token_mint_to, AccountType,
    },
    process_management::{
        process_add_maintainer, process_add_validator, process_change_fee_spec,
        process_claim_validator_fee, process_create_validator_stake_account,
        process_decrease_validator_stake, process_distribute_fees,
        process_increase_validator_stake, process_remove_maintainer, process_remove_validator,
    },
    state::{
        FeeDistribution, FeeRecipients, Lido, Maintainers, Validator, Validators,
        LIDO_CONSTANT_SIZE,
    },
    token::{Lamports, StLamports},
    DEPOSIT_AUTHORITY, FEE_MANAGER_AUTHORITY, RESERVE_AUTHORITY, STAKE_POOL_AUTHORITY,
    VALIDATOR_STAKE_ACCOUNT,
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
    program_id: &Pubkey,
    fee_distribution: FeeDistribution,
    max_validators: u32,
    max_maintainers: u32,
    accounts_raw: &[AccountInfo],
) -> ProgramResult {
    let accounts = InitializeAccountsInfo::try_from_slice(accounts_raw)?;
    let rent = &Rent::from_account_info(accounts.sysvar_rent)?;
    rent_exemption(rent, accounts.stake_pool, AccountType::StakePool)?;
    rent_exemption(rent, accounts.lido, AccountType::Lido)?;
    rent_exemption(rent, accounts.reserve_account, AccountType::ReserveAccount)?;

    let mut lido = deserialize_lido(program_id, accounts.lido)?;
    lido.is_initialized()?;

    let stake_pool = StakePool::try_from_slice(&accounts.stake_pool.data.borrow())?;
    if stake_pool.is_uninitialized() {
        msg!("Provided stake pool not initialized");
        return Err(LidoError::InvalidStakePool.into());
    }

    // Check if fee structure is valid
    Lido::check_valid_minter_program(&accounts.mint_program.key, accounts.insurance_account)?;
    Lido::check_valid_minter_program(&accounts.mint_program.key, accounts.treasury_account)?;
    Lido::check_valid_minter_program(&accounts.mint_program.key, accounts.manager_fee_account)?;

    // Bytes required for maintainers
    let bytes_for_maintainers = Maintainers::required_bytes(max_maintainers as usize);
    // Calculate the expected number of validators
    let expected_max_validators = Validators::maximum_entries(
        accounts
            .lido
            .data_len()
            .checked_sub(LIDO_CONSTANT_SIZE)
            .ok_or(LidoError::CalculationFailure)?
            .checked_sub(bytes_for_maintainers)
            .ok_or(LidoError::CalculationFailure)?,
    );
    if expected_max_validators != max_validators as usize || max_validators == 0 {
        msg!(
            "Incorrect validator list size provided, expected {}, provided {}",
            expected_max_validators,
            max_validators
        );
        return Err(LidoError::UnexpectedMaxValidators.into());
    }
    // Initialize fee structure
    lido.fee_recipients = FeeRecipients {
        insurance_account: *accounts.insurance_account.key,
        treasury_account: *accounts.treasury_account.key,
        manager_account: *accounts.manager_fee_account.key,
    };
    lido.validators = Validators::new(max_validators);

    let (_, reserve_bump_seed) = Pubkey::find_program_address(
        &[&accounts.lido.key.to_bytes()[..32], RESERVE_AUTHORITY],
        program_id,
    );

    let (_, deposit_bump_seed) = Pubkey::find_program_address(
        &[&accounts.lido.key.to_bytes()[..32], DEPOSIT_AUTHORITY],
        program_id,
    );

    let (fee_manager_account, fee_manager_bump_seed) = Pubkey::find_program_address(
        &[&accounts.lido.key.to_bytes()[..32], FEE_MANAGER_AUTHORITY],
        program_id,
    );

    let (stake_pool_authority, stake_pool_authority_bump_seed) = Pubkey::find_program_address(
        &[&accounts.lido.key.to_bytes()[..32], STAKE_POOL_AUTHORITY],
        program_id,
    );

    let pool_to_token_account =
        spl_token::state::Account::unpack_from_slice(&accounts.pool_token_to.data.borrow())?;

    if stake_pool.pool_mint != pool_to_token_account.mint {
        msg!(
            "Pool token to has wrong minter, should be the same as stake pool minter {}",
            stake_pool.pool_mint
        );
        return Err(LidoError::InvalidTokenMinter.into());
    }
    if stake_pool_authority != pool_to_token_account.owner {
        msg!(
            "Wrong stake pool reserve authority: {}",
            pool_to_token_account.owner
        );
        return Err(LidoError::InvalidOwner.into());
    }

    if stake_pool.staker != stake_pool_authority {
        msg!(
            "Stake pool should be managed by the derived address {}",
            &stake_pool_authority
        );
        return Err(LidoError::InvalidManager.into());
    }
    if &stake_pool.manager_fee_account != accounts.fee_token.key {
        msg!("Stake pool's manager_fee should be the same as the token fee account");
        return Err(LidoError::InvalidFeeAccount.into());
    }

    let fee_account =
        spl_token::state::Account::unpack_from_slice(&accounts.fee_token.data.borrow())?;
    if fee_account.owner != fee_manager_account {
        msg!("Fee account has an invalid owner, it should owned by the fee manager authority");
        return Err(LidoError::InvalidOwner.into());
    }

    lido.maintainers = Maintainers::new(max_maintainers);
    lido.stake_pool_account = *accounts.stake_pool.key;
    lido.manager = *accounts.manager.key;
    lido.st_sol_mint_program = *accounts.mint_program.key;
    lido.stake_pool_token_holder = *accounts.pool_token_to.key;
    lido.token_program_id = *accounts.spl_token.key;
    lido.sol_reserve_authority_bump_seed = reserve_bump_seed;
    lido.deposit_authority_bump_seed = deposit_bump_seed;
    lido.stake_pool_authority_bump_seed = stake_pool_authority_bump_seed;
    lido.fee_manager_bump_seed = fee_manager_bump_seed;

    lido.fee_distribution = fee_distribution;

    lido.serialize(&mut *accounts.lido.data.borrow_mut())
        .map_err(|e| e.into())
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

    lido.check_lido_for_deposit(
        accounts.manager.key,
        accounts.stake_pool.key,
        accounts.mint_program.key,
    )?;
    lido.check_token_program_id(accounts.spl_token.key)?;
    check_reserve_authority(accounts.lido, program_id, accounts.reserve_account)?;

    lido.check_stake_pool(accounts.stake_pool)?;

    let stake_pool = StakePool::try_from_slice(&accounts.stake_pool.data.borrow())?;

    let rent = &Rent::from_account_info(accounts.sysvar_rent)?;
    let pool_to_token_account =
        spl_token::state::Account::unpack_from_slice(&accounts.pool_token_to.data.borrow())?;

    let total_lamports = calc_total_lamports(
        &stake_pool,
        &pool_to_token_account,
        accounts.reserve_account,
        rent,
    )?;
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
        accounts.mint_program.clone(),
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

    let minium_stake_balance =
        Lamports(rent.minimum_balance(std::mem::size_of::<stake_program::StakeState>()));
    if amount < minium_stake_balance {
        return Err(LidoError::InvalidAmount.into());
    }

    let available_reserve_amount = get_reserve_available_amount(accounts.reserve, rent)?;
    if amount > available_reserve_amount {
        msg!("The requested amount {} is greater than the available amount {}, considering rent-exemption", amount, available_reserve_amount);
        return Err(LidoError::AmountExceedsReserve.into());
    }

    let (validator_key, validator) = lido.validators.get_mut(&accounts.validator.key)?;

    // We will create a new fresh stake account for this validator.
    // TODO: Merge into the preceding stake account, if possible, such that we
    // don't create a new account per `StakeDeposit`, but only per epoch.
    let (stake_addr, stake_addr_bump_seed) = Validator::find_stake_account_address(
        program_id,
        accounts.lido.key,
        validator_key,
        validator.stake_accounts_seed_end,
    );
    if &stake_addr != accounts.stake_account_end.key {
        return Err(LidoError::InvalidStakeAccount.into());
    }

    let solido_address_bytes = accounts.lido.key.to_bytes();
    let reserve_authority_seed: &[&[_]] = &[&solido_address_bytes, RESERVE_AUTHORITY][..];
    let (reserve_authority, _) = Pubkey::find_program_address(reserve_authority_seed, program_id);

    if accounts.reserve.key != &reserve_authority {
        return Err(LidoError::InvalidReserveAuthority.into());
    }

    let reserve_account_bump_seed = [lido.sol_reserve_authority_bump_seed];
    let stake_account_bump_seed = [stake_addr_bump_seed];
    let validator_address_bytes = accounts.validator.key.to_bytes();

    let reserve_account_seeds = &[
        &solido_address_bytes,
        &RESERVE_AUTHORITY[..],
        &reserve_account_bump_seed[..],
    ][..];
    let stake_account_seeds = &[
        &solido_address_bytes,
        &validator_address_bytes,
        &VALIDATOR_STAKE_ACCOUNT[..],
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
            accounts.validator.key,
        ),
        &[
            accounts.stake_account_end.clone(),
            accounts.validator.clone(),
            accounts.sysvar_clock.clone(),
            accounts.stake_history.clone(),
            accounts.stake_program_config.clone(),
            accounts.deposit_authority.clone(),
        ],
        &[&[
            &accounts.lido.key.to_bytes()[..],
            DEPOSIT_AUTHORITY,
            &[lido.deposit_authority_bump_seed],
        ]],
    )?;

    // Read the new balance. If there was balance in the stake account
    // already, then the amount we actually staked might be higher than the
    // amount transferred from the reserve.
    let amount_staked = Lamports(accounts.stake_account_end.lamports());

    // Update the total SOL that is activating for this validator.
    validator.stake_accounts_balance =
        (validator.stake_accounts_balance + amount_staked).ok_or(LidoError::CalculationFailure)?;

    // We now consumed this stake account, bump the index.
    validator.stake_accounts_seed_end += 1;

    lido.serialize(&mut *accounts.lido.data.borrow_mut())?;

    Ok(())
}

pub fn process_deposit_active_stake_to_pool(
    program_id: &Pubkey,
    raw_accounts: &[AccountInfo],
) -> ProgramResult {
    let accounts = DepositActiveStakeToPoolAccountsInfo::try_from_slice(raw_accounts)?;

    let _rent = &Rent::from_account_info(accounts.sysvar_rent)?;
    let mut lido = deserialize_lido(program_id, accounts.lido)?;

    lido.check_stake_pool(accounts.stake_pool)?;
    lido.check_maintainer(accounts.maintainer)?;

    let (validator_key, validator) = lido.validators.get_mut(&accounts.validator.key)?;

    // A deposit to the stake pool always deposits from the begin of the range
    // of stake accounts. The `begin` index holds the oldest stake account.
    let (stake_addr, stake_addr_bump_seed) = Validator::find_stake_account_address(
        program_id,
        accounts.lido.key,
        validator_key,
        validator.stake_accounts_seed_begin,
    );
    if &stake_addr != accounts.stake_account_begin.key {
        return Err(LidoError::InvalidStakeAccount.into());
    }

    if &lido.stake_pool_token_holder != accounts.pool_token_to.key {
        msg!("Invalid stake pool token");
        return Err(LidoError::InvalidPoolToken.into());
    }

    // Before we put the stake account in the pool, record how much SOL it held,
    // because that SOL is now no longer activating, so we need to update the
    // `Validator` instance.
    let amount_staked = Lamports(accounts.stake_account_begin.lamports());
    validator.stake_accounts_balance =
        (validator.stake_accounts_balance - amount_staked).ok_or(LidoError::CalculationFailure)?;

    // We now consumed this stake account, bump the index.
    validator.stake_accounts_seed_begin += 1;

    lido.serialize(&mut *accounts.lido.data.borrow_mut())?;

    let stake_account_bump_seed = [stake_addr_bump_seed];
    let solido_address_bytes = accounts.lido.key.to_bytes();
    let validator_address_bytes = accounts.validator.key.to_bytes();
    let stake_account_seeds = &[
        &solido_address_bytes,
        &validator_address_bytes,
        &VALIDATOR_STAKE_ACCOUNT[..],
        &stake_account_bump_seed[..],
    ][..];

    // The stake pool should check that the account we deposit is actually a
    // fully active stake account, and not still activating.

    invoke_signed(
        &stake_pool_deposit(
            &accounts.stake_pool_program.key,
            &StakePoolDepositAccountsMeta {
                stake_pool: *accounts.stake_pool.key,
                validator_list_storage: *accounts.stake_pool_validator_list.key,
                deposit_authority: *accounts.deposit_authority.key,
                stake_pool_withdraw_authority: *accounts.stake_pool_withdraw_authority.key,
                deposit_stake_address: *accounts.stake_account_begin.key,
                validator_stake_account: *accounts.stake_pool_validator_stake_account.key,
                pool_tokens_to: *accounts.pool_token_to.key,
                pool_mint: *accounts.stake_pool_mint.key,
            },
        )?,
        &[
            accounts.stake_pool_program.clone(),
            accounts.stake_pool.clone(),
            accounts.stake_pool_validator_list.clone(),
            accounts.deposit_authority.clone(),
            accounts.stake_pool_withdraw_authority.clone(),
            accounts.stake_account_begin.clone(),
            accounts.stake_pool_validator_stake_account.clone(),
            accounts.pool_token_to.clone(),
            accounts.stake_pool_mint.clone(),
            accounts.spl_token.clone(),
        ],
        &[stake_account_seeds],
    )?;

    Ok(())
}

pub fn process_withdraw(
    _program_id: &Pubkey,
    _pool_tokens: StLamports,
    _accounts: &[AccountInfo],
) -> ProgramResult {
    // TODO
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
        LidoInstruction::DepositActiveStakeToPool => {
            process_deposit_active_stake_to_pool(program_id, accounts)
        }
        LidoInstruction::Withdraw { amount } => process_withdraw(program_id, amount, accounts),
        LidoInstruction::DistributeFees => process_distribute_fees(program_id, accounts),
        LidoInstruction::ClaimValidatorFees => process_claim_validator_fee(program_id, accounts),
        LidoInstruction::ChangeFeeSpec {
            new_fee_distribution,
        } => process_change_fee_spec(program_id, new_fee_distribution, accounts),
        LidoInstruction::CreateValidatorStakeAccount => {
            process_create_validator_stake_account(program_id, accounts)
        }
        LidoInstruction::AddValidator => process_add_validator(program_id, accounts),
        LidoInstruction::RemoveValidator => process_remove_validator(program_id, accounts),
        LidoInstruction::AddMaintainer => process_add_maintainer(program_id, accounts),
        LidoInstruction::RemoveMaintainer => process_remove_maintainer(program_id, accounts),
        LidoInstruction::IncreaseValidatorStake { lamports } => {
            process_increase_validator_stake(program_id, lamports, accounts)
        }
        LidoInstruction::DecreaseValidatorStake { lamports } => {
            process_decrease_validator_stake(program_id, lamports, accounts)
        }
    }
}
