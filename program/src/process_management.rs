use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    borsh::try_from_slice_unchecked,
    entrypoint::ProgramResult,
    msg,
    program::invoke_signed,
    program_pack::Pack,
    pubkey::Pubkey,
};
use spl_stake_pool::{
    error::StakePoolError,
    instruction::{add_validator_to_pool, create_validator_stake_account},
    state::{StakePool, ValidatorList},
};

use crate::{
    error::LidoError,
    instruction::{
        AddValidatorInfo, ChangeFeeDistributionInfo, CreateValidatorStakeAccountInfo,
        DistributeFeesInfo,
    },
    logic::{token_mint_to, transfer_to},
    state::{FeeDistribution, Lido, ValidatorCredit, ValidatorCreditAccounts},
    FEE_MANAGER_AUTHORITY, RESERVE_AUTHORITY, STAKE_POOL_AUTHORITY,
};

pub fn process_create_validator_stake_account(
    program_id: &Pubkey,
    accounts_raw: &[AccountInfo],
) -> ProgramResult {
    let accounts = CreateValidatorStakeAccountInfo::try_from_slice(accounts_raw)?;
    let (stake_pool_authority, stake_pool_authority_bump_seed) = Pubkey::find_program_address(
        &[&accounts.lido.key.to_bytes()[..32], STAKE_POOL_AUTHORITY],
        program_id,
    );
    if &stake_pool_authority != accounts.staker.key {
        msg!("Wrong stake pool staker");
        return Err(LidoError::InvalidStaker.into());
    }

    invoke_signed(
        &create_validator_stake_account(
            &spl_stake_pool::id(),
            accounts.stake_pool.key,
            accounts.staker.key,
            accounts.funder.key,
            accounts.stake_account.key,
            accounts.validator.key,
        )?,
        &[
            accounts.stake_pool_program.clone(),
            accounts.staker.clone(),
            accounts.funder.clone(),
            accounts.stake_account.clone(),
            accounts.validator.clone(),
            accounts.sysvar_rent.clone(),
            accounts.sysvar_clock.clone(),
            accounts.sysvar_stake_history.clone(),
            accounts.stake_program_config.clone(),
            accounts.system_program.clone(),
            accounts.stake_program.clone(),
        ],
        &[&[
            &accounts.lido.key.to_bytes()[..32],
            STAKE_POOL_AUTHORITY,
            &[stake_pool_authority_bump_seed],
        ]],
    )
}

pub fn process_change_fee_distribution(
    program_id: &Pubkey,
    accounts_raw: &[AccountInfo],
) -> ProgramResult {
    let accounts = ChangeFeeDistributionInfo::try_from_slice(accounts_raw)?;
    // TODO(fynn): Remove new_fee_distribution in favour of keeping the state in the Lido's account
    if accounts.lido.owner != program_id || accounts.new_fee_distribution.owner != program_id {
        msg!("State has invalid owner");
        return Err(LidoError::InvalidOwner.into());
    }

    let mut lido = try_from_slice_unchecked::<Lido>(&accounts.lido.data.borrow())?;
    if &lido.fee_distribution != accounts.current_fee_distribution.key {
        msg!("Invalid current fee distribution account");
        return Err(LidoError::InvalidFeeDistributionAccount.into());
    }

    let current_fee_distribution = try_from_slice_unchecked::<FeeDistribution>(
        &accounts.current_fee_distribution.data.borrow(),
    )?;

    if &lido.validator_credit_accounts != accounts.validator_credit_accounts.key {
        msg!("Invalid validators credit accounts");
        return Err(LidoError::InvalidValidatorCreditAccount.into());
    }

    let new_fee_distribution =
        try_from_slice_unchecked::<FeeDistribution>(&accounts.new_fee_distribution.data.borrow())?;
    new_fee_distribution.check_sum()?;

    lido.fee_distribution = *accounts.new_fee_distribution.key;

    lido.serialize(&mut *accounts.lido.data.borrow_mut())
        .map_err(|e| e.into())
}

pub fn process_add_validator(program_id: &Pubkey, accounts_raw: &[AccountInfo]) -> ProgramResult {
    // TODO(fynn): Change in favour of integrating the state to Lido's
    let accounts = AddValidatorInfo::try_from_slice(accounts_raw)?;

    let lido = try_from_slice_unchecked::<Lido>(&accounts.lido.data.borrow())?;
    if &lido.stake_pool_account != accounts.stake_pool.key {
        msg!("Invalid stake pool");
        return Err(LidoError::InvalidStakePool.into());
    }

    if &lido.validator_credit_accounts != accounts.validator_credit_accounts.key {
        msg!("Invalid validator credit accounts");
        return Err(LidoError::InvalidValidatorCreditAccount.into());
    }

    let validator_token_account = spl_token::state::Account::unpack_from_slice(
        &accounts.validator_token_account.data.borrow(),
    )?;
    if lido.st_sol_mint_program != validator_token_account.mint {
        msg!(
            "Validator account minter should be the same as Lido minter {}",
            lido.st_sol_mint_program
        );
        return Err(LidoError::InvalidTokenMinter.into());
    }

    // TODO: Check stake pool manager authority

    invoke_signed(
        &add_validator_to_pool(
            accounts.stake_pool_program_id.key,
            accounts.stake_pool.key,
            accounts.stake_pool_manager_authority.key,
            accounts.stake_pool_withdraw_authority.key,
            accounts.stake_pool_validator_list.key,
            accounts.stake_account.key,
        )?,
        &[
            accounts.stake_pool_program_id.clone(),
            accounts.stake_pool.clone(),
            accounts.stake_pool_manager_authority.clone(),
            accounts.stake_pool_withdraw_authority.clone(),
            accounts.stake_pool_validator_list.clone(),
            accounts.stake_account.clone(),
            accounts.sysvar_clock.clone(),
            accounts.sysvar_stake_history.clone(),
            accounts.sysvar_stake_program.clone(),
        ],
        &[&[
            &accounts.lido.key.to_bytes()[..32],
            STAKE_POOL_AUTHORITY,
            &[lido.stake_pool_authority_bump_seed],
        ]],
    )?;

    let mut validator_credit_accounts = try_from_slice_unchecked::<ValidatorCreditAccounts>(
        &accounts.validator_credit_accounts.data.borrow(),
    )?;
    // If the condition below is false, the stake pool operation should have failed, but
    // we double check to be sure
    if validator_credit_accounts.validator_accounts.len() as u32
        == validator_credit_accounts.max_validators
    {
        msg!("Maximum number of validators reached");
        return Err(LidoError::UnexpectedValidatorCreditAccountSize.into());
    }

    validator_credit_accounts
        .validator_accounts
        .push(ValidatorCredit {
            address: *accounts.validator_token_account.key,
            amount: 0,
        });

    validator_credit_accounts
        .serialize(&mut *accounts.validator_credit_accounts.data.borrow_mut())
        .map_err(|err| err.into())
}

/// TODO
pub fn process_remove_validator(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    unimplemented!()
}

/// TODO
pub fn process_claim_validators_fee(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    unimplemented!()
}

pub fn process_distribute_fees(program_id: &Pubkey, accounts_raw: &[AccountInfo]) -> ProgramResult {
    let accounts = DistributeFeesInfo::try_from_slice(accounts_raw)?;

    let lido = try_from_slice_unchecked::<Lido>(&accounts.lido.data.borrow())?;

    if &lido.stake_pool_account != accounts.stake_pool.key {
        msg!("Invalid stake pool");
        return Err(LidoError::InvalidStakePool.into());
    }
    if &lido.validator_credit_accounts != accounts.validator_credit_accounts.key {
        msg!("Wrong validator credit accounts");
        return Err(LidoError::InvalidValidatorCreditAccount.into());
    }
    if &lido.fee_distribution != accounts.fee_distribution.key {
        msg!("Wrong fee distribution");
        return Err(LidoError::InvalidFeeDistributionAccount.into());
    }

    let stake_pool = StakePool::try_from_slice(&accounts.stake_pool.data.borrow())?;
    if &stake_pool.validator_list != accounts.stake_pool_validator_list.key {
        msg!("Invalid validators list from StakePool");
        return Err(StakePoolError::InvalidValidatorStakeList.into());
    }
    if &stake_pool.manager_fee_account != accounts.stake_pool_fee_account.key {
        msg!("Invalid fee account from StakePool");
        return Err(StakePoolError::InvalidFeeAccount.into());
    }
    let stake_pool_fee_account = spl_token::state::Account::unpack_from_slice(
        &accounts.stake_pool_fee_account.data.borrow(),
    )?;

    let validator_list = try_from_slice_unchecked::<ValidatorList>(
        &accounts.stake_pool_validator_list.data.borrow(),
    )?;

    let fee_distribution =
        try_from_slice_unchecked::<FeeDistribution>(&accounts.fee_distribution.data.borrow())?;

    let token_shares = fee_distribution.calculate_token_amounts(
        stake_pool_fee_account.amount,
        validator_list.validators.len() as u32,
    )?;

    let mut validator_credit_accounts = try_from_slice_unchecked::<ValidatorCreditAccounts>(
        &accounts.validator_credit_accounts.data.borrow(),
    )?;

    // Send all tokens to Lido token holder
    transfer_to(
        accounts.lido.key,
        accounts.spl_token.clone(),
        accounts.stake_pool_manager_fee_account.clone(),
        accounts.token_holder_stake_pool.clone(),
        accounts.fee_manager_account.clone(),
        FEE_MANAGER_AUTHORITY,
        lido.fee_manager_bump_seed,
        stake_pool_fee_account.amount,
    )?;

    // Mint tokens for insurance
    token_mint_to(
        accounts.lido.key,
        accounts.spl_token.clone(),
        accounts.mint_program.clone(),
        accounts.insurance_account.clone(),
        accounts.reserve_authority.clone(),
        RESERVE_AUTHORITY,
        lido.sol_reserve_authority_bump_seed,
        token_shares.insurance_amount,
    )?;
    // Mint tokens for treasury
    token_mint_to(
        accounts.lido.key,
        accounts.spl_token.clone(),
        accounts.mint_program.clone(),
        accounts.treasury_account.clone(),
        accounts.reserve_authority.clone(),
        RESERVE_AUTHORITY,
        lido.sol_reserve_authority_bump_seed,
        token_shares.treasury_amount,
    )?;
    // Mint tokens for manager
    token_mint_to(
        accounts.lido.key,
        accounts.spl_token.clone(),
        accounts.mint_program.clone(),
        accounts.manager.clone(),
        accounts.reserve_authority.clone(),
        RESERVE_AUTHORITY,
        lido.sol_reserve_authority_bump_seed,
        token_shares.manager_amount,
    )?;

    // Update validator list that can be claimed at a later time
    for idx in 0..validator_list.validators.len() {
        validator_credit_accounts.validator_accounts[idx].amount +=
            token_shares.each_validator_amount;
    }
    validator_credit_accounts
        .serialize(&mut *accounts.validator_credit_accounts.data.borrow_mut())
        .map_err(|err| err.into())
}

/// TODO
/// Called by the validator, changes the fee account which the validator
/// receives tokens
pub fn process_change_validator_fee_account(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    unimplemented!()
}
