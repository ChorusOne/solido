//! Program state processor

use solana_program::program_pack::Pack;
use spl_stake_pool::{stake_program, state::StakePool};

use crate::{
    error::LidoError,
    instruction::{
        stake_pool_deposit, DelegateDepositAccountsInfo, DepositAccountsInfo,
        InitializeAccountsInfo, LidoInstruction, StakePoolDelegateAccountsInfo,
        StakePoolDepositAccountsMeta,
    },
    logic::{
        calc_stakepool_lamports, calc_total_lamports, check_reserve_authority, rent_exemption,
        AccountType,
    },
    process_management::{
        process_add_validator, process_change_fee_distribution, process_remove_validator,
    },
    state::{FeeDistribution, Lido, ValidatorCreditAccounts},
    DEPOSIT_AUTHORITY, FEE_MANAGER_AUTHORITY, RESERVE_AUTHORITY, STAKE_POOL_MANAGER,
    STAKE_POOL_TOKEN_RESERVE_AUTHORITY,
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
    spl_stake_pool::borsh::try_from_slice_unchecked,
};

fn get_stake_state(
    stake_account_info: &AccountInfo,
) -> Result<(stake_program::Meta, stake_program::Stake), ProgramError> {
    let stake_state =
        try_from_slice_unchecked::<stake_program::StakeState>(&stake_account_info.data.borrow())?;
    match stake_state {
        stake_program::StakeState::Stake(meta, stake) => Ok((meta, stake)),
        _ => Err(LidoError::WrongStakeState.into()),
    }
}

/// Program state handler.
pub fn process_initialize(
    program_id: &Pubkey,
    max_validators: u32,
    accounts_raw: &[AccountInfo],
) -> ProgramResult {
    let accounts = InitializeAccountsInfo::try_from_slice(accounts_raw)?;

    let rent = &Rent::from_account_info(accounts.sysvar_rent)?;
    rent_exemption(rent, accounts.stake_pool, AccountType::StakePool)?;
    rent_exemption(rent, accounts.lido, AccountType::Lido)?;
    rent_exemption(
        rent,
        accounts.fee_distribution,
        AccountType::FeeDistribution,
    )?;
    rent_exemption(
        rent,
        accounts.validator_credit_accounts,
        AccountType::ValidatorCreditAccounts,
    )?;

    let mut lido = try_from_slice_unchecked::<Lido>(&accounts.lido.data.borrow())?;
    lido.is_initialized()?;

    let stake_pool = StakePool::try_from_slice(&accounts.stake_pool.data.borrow())?;
    if stake_pool.is_uninitialized() {
        msg!("Provided stake pool not initialized");
        return Err(LidoError::InvalidStakePool.into());
    }
    let pool_to_token_account =
        spl_token::state::Account::unpack_from_slice(&accounts.pool_token_to.data.borrow())?;

    if stake_pool.pool_mint != pool_to_token_account.mint {
        msg!(
            "Pool token to has wrong minter, should be the same as stake pool minter {}",
            stake_pool.pool_mint
        );
        return Err(LidoError::InvalidTokenMinter.into());
    }

    // Check fees structure
    // if &stake_pool.staker != program_id {
    //     msg!("Stake pool's staker should be the Lido program");
    //     return Err(LidoError::InvalidOwner.into());
    // }
    let mut fee_distribution =
        try_from_slice_unchecked::<FeeDistribution>(&accounts.fee_distribution.data.borrow())?;
    fee_distribution.check_sum()?;

    let expected_max_validators = ValidatorCreditAccounts::maximum_byte_capacity(
        accounts.validator_credit_accounts.data_len(),
    );
    if expected_max_validators != max_validators as usize || max_validators == 0 {
        msg!(
            "Incorrect validator list size provided, expected {}, provided {}",
            expected_max_validators,
            max_validators
        );
        return Err(LidoError::UnexpectedValidatorCreditAccountSize.into());
    }
    let mut validator_credit_accounts = try_from_slice_unchecked::<ValidatorCreditAccounts>(
        &accounts.validator_credit_accounts.data.borrow(),
    )?;
    validator_credit_accounts.max_validators = max_validators;
    validator_credit_accounts.validator_accounts.clear();
    fee_distribution.validator_list_account = *accounts.validator_credit_accounts.key;

    let (_, reserve_bump_seed) = Pubkey::find_program_address(
        &[&accounts.lido.key.to_bytes()[..32], RESERVE_AUTHORITY],
        program_id,
    );

    let (_, deposit_bump_seed) = Pubkey::find_program_address(
        &[&accounts.lido.key.to_bytes()[..32], DEPOSIT_AUTHORITY],
        program_id,
    );

    let (_, token_reserve_bump_seed) = Pubkey::find_program_address(
        &[
            &accounts.lido.key.to_bytes()[..32],
            STAKE_POOL_TOKEN_RESERVE_AUTHORITY,
        ],
        program_id,
    );

    let (fee_manager_account, _fee_manager_bump_seed) = Pubkey::find_program_address(
        &[&accounts.lido.key.to_bytes()[..32], FEE_MANAGER_AUTHORITY],
        program_id,
    );

    let (stake_pool_manager, stake_pool_authority_bump_seed) = Pubkey::find_program_address(
        &[&accounts.lido.key.to_bytes()[..32], STAKE_POOL_MANAGER],
        program_id,
    );
    if stake_pool.staker != stake_pool_manager {
        msg!(
            "Stake pool should be managed by the derived address {}",
            &stake_pool_manager
        );
        return Err(LidoError::InvalidManager.into());
    }

    let fee_account =
        spl_token::state::Account::unpack_from_slice(&accounts.fee_token.data.borrow())?;
    if fee_account.owner != fee_manager_account {
        msg!("Fee account has an invalid owner, it should owned by the fee manager authority");
        return Err(LidoError::InvalidOwner.into());
    }

    validator_credit_accounts
        .serialize(&mut *accounts.validator_credit_accounts.data.borrow_mut())?;
    fee_distribution.serialize(&mut *accounts.fee_distribution.data.borrow_mut())?;
    lido.stake_pool_account = *accounts.stake_pool.key;
    lido.manager = *accounts.manager.key;
    lido.lsol_mint_program = *accounts.mint_program.key;
    lido.sol_reserve_authority_bump_seed = reserve_bump_seed;
    lido.deposit_authority_bump_seed = deposit_bump_seed;
    lido.token_reserve_authority_bump_seed = token_reserve_bump_seed;
    lido.stake_pool_authority_bump_seed = stake_pool_authority_bump_seed;
    lido.token_program_id = *accounts.spl_token.key;
    lido.pool_token_to = *accounts.pool_token_to.key;
    lido.fee_distribution = *accounts.fee_distribution.key;

    lido.serialize(&mut *accounts.lido.data.borrow_mut())
        .map_err(|e| e.into())
}

pub fn process_deposit(
    program_id: &Pubkey,
    amount: u64,
    accounts_raw: &[AccountInfo],
) -> ProgramResult {
    let accounts = DepositAccountsInfo::try_from_slice(accounts_raw)?;

    if amount == 0 {
        msg!("Amount must be greater than zero");
        return Err(ProgramError::InvalidArgument);
    }

    if accounts.user.lamports() < amount {
        return Err(LidoError::InvalidAmount.into());
    }

    let mut lido = Lido::try_from_slice(&accounts.lido.data.borrow())?;

    lido.check_lido_for_deposit(
        accounts.manager.key,
        accounts.stake_pool.key,
        accounts.mint_program.key,
    )?;
    lido.check_token_program_id(accounts.spl_token.key)?;
    check_reserve_authority(accounts.lido, program_id, accounts.reserve_authority)?;

    if &lido.stake_pool_account != accounts.stake_pool.key {
        msg!("Invalid stake pool");
        return Err(LidoError::InvalidStakePool.into());
    }
    let stake_pool = StakePool::try_from_slice(&accounts.stake_pool.data.borrow())?;
    if !stake_pool.is_valid() {
        msg!("Invalid stake pool");
        return Err(LidoError::InvalidStakePool.into());
    }
    if &stake_pool.token_program_id != accounts.spl_token.key {
        msg!("Invalid token program");
        return Err(LidoError::InvalidTokenProgram.into());
    }

    if &lido.pool_token_to != accounts.pool_token_to.key {
        msg!("Invalid stake pool token");
        return Err(LidoError::InvalidPoolToken.into());
    }

    let reserve_lamports = accounts.reserve_authority.lamports();

    let pool_to_token_account =
        spl_token::state::Account::unpack_from_slice(&accounts.pool_token_to.data.borrow())?;

    // stake_pool_total_sol * stake_pool_token(pool_token_to_info)/stake_pool_total_tokens
    let stake_pool_lamports = calc_stakepool_lamports(stake_pool, pool_to_token_account)?;

    let total_lamports = calc_total_lamports(reserve_lamports, stake_pool_lamports);
    invoke(
        &system_instruction::transfer(accounts.user.key, accounts.reserve_authority.key, amount),
        &[
            accounts.user.clone(),
            accounts.reserve_authority.clone(),
            accounts.system_program.clone(),
        ],
    )?;

    let lsol_amount = lido
        .calc_pool_tokens_for_deposit(amount, total_lamports)
        .ok_or(LidoError::CalculationFailure)?;

    let total_lsol = lido.lsol_total_shares + lsol_amount;

    let ix = spl_token::instruction::mint_to(
        accounts.spl_token.key,
        accounts.mint_program.key,
        accounts.recipient.key,
        accounts.reserve_authority.key,
        &[],
        lsol_amount,
    )?;

    let me_bytes = accounts.lido.key.to_bytes();
    let authority_signature_seeds = [
        &me_bytes[..32],
        RESERVE_AUTHORITY,
        &[lido.sol_reserve_authority_bump_seed],
    ];
    let signers = &[&authority_signature_seeds[..]];
    invoke_signed(
        &ix,
        &[
            accounts.mint_program.clone(),
            accounts.recipient.clone(),
            accounts.reserve_authority.clone(),
            accounts.spl_token.clone(),
        ],
        signers,
    )?;

    lido.lsol_total_shares = total_lsol;

    lido.serialize(&mut *accounts.lido.data.borrow_mut())
        .map_err(|e| e.into())
}

pub fn process_delegate_deposit(
    program_id: &Pubkey,
    amount: u64,
    raw_accounts: &[AccountInfo],
) -> ProgramResult {
    let accounts = DelegateDepositAccountsInfo::try_from_slice(raw_accounts)?;

    let rent = &Rent::from_account_info(accounts.sysvar_rent)?;
    let lido = Lido::try_from_slice(&accounts.lido.data.borrow())?;

    let (to_pubkey, stake_bump_seed) =
        Pubkey::find_program_address(&[&accounts.validator.key.to_bytes()[..32]], program_id);
    if &to_pubkey != accounts.stake.key {
        return Err(LidoError::InvalidStaker.into());
    }

    let me_bytes = accounts.lido.key.to_bytes();
    let reserve_authority_seed: &[&[_]] = &[&me_bytes, RESERVE_AUTHORITY][..];
    let (reserve_authority, _) = Pubkey::find_program_address(reserve_authority_seed, program_id);

    if accounts.reserve.key != &reserve_authority {
        return Err(LidoError::InvalidReserveAuthority.into());
    }

    if amount < rent.minimum_balance(std::mem::size_of::<stake_program::StakeState>()) {
        return Err(LidoError::InvalidAmount.into());
    }

    // TODO: Reference more validators

    let authority_signature_seeds: &[&[_]] = &[
        &me_bytes,
        &RESERVE_AUTHORITY,
        &[lido.sol_reserve_authority_bump_seed],
    ];

    let validator_stake_seeds: &[&[_]] =
        &[&accounts.validator.key.to_bytes()[..32], &[stake_bump_seed]];

    // Check if the stake_info exists
    if get_stake_state(accounts.stake).is_ok() {
        return Err(LidoError::WrongStakeState.into());
    }

    invoke_signed(
        &system_instruction::create_account(
            accounts.reserve.key,
            accounts.stake.key,
            amount,
            std::mem::size_of::<stake_program::StakeState>() as u64,
            &stake_program::id(),
        ),
        // &[reserve_info.clone(), stake_info.clone()],
        &[
            accounts.reserve.clone(),
            accounts.stake.clone(),
            accounts.system_program.clone(),
        ],
        &[&authority_signature_seeds, &validator_stake_seeds],
    )?;

    invoke(
        &stake_program::initialize(
            accounts.stake.key,
            &stake_program::Authorized {
                staker: *accounts.deposit_authority.key,
                withdrawer: *accounts.deposit_authority.key,
            },
            &stake_program::Lockup::default(),
        ),
        &[
            accounts.stake.clone(),
            accounts.sysvar_rent.clone(),
            accounts.stake_program.clone(),
        ],
    )?;

    invoke_signed(
        &stake_program::delegate_stake(
            accounts.stake.key,
            accounts.deposit_authority.key,
            accounts.validator.key,
        ),
        &[
            accounts.stake.clone(),
            accounts.validator.clone(),
            accounts.sysvar_clock.clone(),
            accounts.stake_history.clone(),
            accounts.stake_program_config.clone(),
            accounts.deposit_authority.clone(),
        ],
        &[&[
            &accounts.lido.key.to_bytes()[..32],
            DEPOSIT_AUTHORITY,
            &[lido.deposit_authority_bump_seed],
        ]],
    )
}

pub fn process_stake_pool_delegate(
    program_id: &Pubkey,
    raw_accounts: &[AccountInfo],
) -> ProgramResult {
    let accounts = StakePoolDelegateAccountsInfo::try_from_slice(raw_accounts)?;

    let _rent = &Rent::from_account_info(accounts.sysvar_rent)?;
    let lido = Lido::try_from_slice(&accounts.lido.data.borrow())?;

    if &lido.stake_pool_account != accounts.stake_pool.key {
        msg!("Invalid stake pool");
        return Err(LidoError::InvalidStakePool.into());
    }

    let (to_pubkey, _) =
        Pubkey::find_program_address(&[&accounts.validator.key.to_bytes()[..32]], program_id);

    let (stake_pool_token_reserve_authority, _) = Pubkey::find_program_address(
        &[
            &accounts.lido.key.to_bytes()[..32],
            STAKE_POOL_TOKEN_RESERVE_AUTHORITY,
        ],
        program_id,
    );

    if &to_pubkey != accounts.stake.key {
        return Err(LidoError::InvalidStaker.into());
    }

    let pool_token_account =
        spl_token::state::Account::unpack_from_slice(&accounts.pool_token_to.data.borrow())?;

    if &lido.pool_token_to != accounts.pool_token_to.key {
        msg!("Invalid stake pool token");
        return Err(LidoError::InvalidPoolToken.into());
    }

    if stake_pool_token_reserve_authority != pool_token_account.owner {
        msg!(
            "Wrong stake pool reserve authority: {}",
            pool_token_account.owner
        );
        return Err(LidoError::InvalidOwner.into());
    }

    invoke_signed(
        &stake_pool_deposit(
            &accounts.stake_pool_program.key,
            &StakePoolDepositAccountsMeta {
                stake_pool: *accounts.stake_pool.key,
                validator_list_storage: *accounts.stake_pool_validator_list.key,
                deposit_authority: *accounts.deposit_authority.key,
                stake_pool_withdraw_authority: *accounts.stake_pool_withdraw_authority.key,
                deposit_stake_address: *accounts.stake.key,
                validator_stake_account: *accounts.stake_pool_validator_stake_account.key,
                pool_tokens_to: *accounts.pool_token_to.key,
                pool_mint: *accounts.stake_pool_mint.key,
            },
        ),
        &[
            accounts.stake_pool_program.clone(),
            accounts.stake_pool.clone(),
            accounts.stake_pool_validator_list.clone(),
            accounts.deposit_authority.clone(),
            accounts.stake_pool_withdraw_authority.clone(),
            accounts.stake.clone(),
            accounts.stake_pool_validator_stake_account.clone(),
            accounts.pool_token_to.clone(),
            accounts.stake_pool_mint.clone(),
            accounts.spl_token.clone(),
        ],
        &[&[
            &accounts.lido.key.to_bytes()[..32],
            DEPOSIT_AUTHORITY,
            &[lido.deposit_authority_bump_seed],
        ]],
    )?;
    Ok(())
}

pub fn process_withdraw(
    _program_id: &Pubkey,
    _pool_tokens: u64,
    _accounts: &[AccountInfo],
) -> ProgramResult {
    Ok(())
}

/// Processes [Instruction](enum.Instruction.html).
pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], input: &[u8]) -> ProgramResult {
    let instruction = LidoInstruction::try_from_slice(input)?;
    match instruction {
        LidoInstruction::Initialize { max_validators } => {
            process_initialize(program_id, max_validators, accounts)
        }
        LidoInstruction::Deposit { amount } => process_deposit(program_id, amount, accounts),
        LidoInstruction::DelegateDeposit { amount } => {
            process_delegate_deposit(program_id, amount, accounts)
        }
        LidoInstruction::StakePoolDelegate => process_stake_pool_delegate(program_id, accounts),
        LidoInstruction::Withdraw { amount } => process_withdraw(program_id, amount, accounts),
        LidoInstruction::ChangeFeeDistribution => {
            process_change_fee_distribution(program_id, accounts)
        }
        LidoInstruction::AddValidator => process_add_validator(program_id, accounts),
        LidoInstruction::RemoveValidator => process_remove_validator(program_id, accounts),
    }
}
