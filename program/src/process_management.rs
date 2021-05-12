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
    instruction::add_validator_to_pool,
    state::{StakePool, ValidatorList},
};

use crate::{
    error::LidoError,
    logic::{token_mint_to, transfer_to},
    state::{FeeDistribution, Lido, ValidatorCredit, ValidatorCreditAccounts},
    FEE_MANAGER_AUTHORITY, RESERVE_AUTHORITY, STAKE_POOL_MANAGER,
};

pub fn process_change_fee_distribution(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();

    let lido_info = next_account_info(account_info_iter)?;
    let manager = next_account_info(account_info_iter)?;
    let current_fee_distribution_info = next_account_info(account_info_iter)?;
    let new_fee_distribution_info = next_account_info(account_info_iter)?;
    let validator_credit_accounts_info = next_account_info(account_info_iter)?;

    if !manager.is_signer {
        msg!("Message needs to be signed by Lido's manager");
        return Err(LidoError::InvalidOwner.into());
    }
    // TODO(fynn): Remove new_fee_distribution in favour of keeping the state in the Lido's account
    if lido_info.owner != program_id || new_fee_distribution_info.owner != program_id {
        msg!("State has invalid owner");
        return Err(LidoError::InvalidOwner.into());
    }

    let mut lido = try_from_slice_unchecked::<Lido>(&lido_info.data.borrow())?;
    if &lido.fee_distribution != current_fee_distribution_info.key {
        msg!("Invalid current fee distribution account");
        return Err(LidoError::InvalidFeeDistributionAccount.into());
    }

    let current_fee_distribution =
        try_from_slice_unchecked::<FeeDistribution>(&current_fee_distribution_info.data.borrow())?;

    if &lido.validator_credit_accounts != validator_credit_accounts_info.key {
        msg!("Invalid validators credit accounts");
        return Err(LidoError::InvalidValidatorCreditAccount.into());
    }

    let new_fee_distribution =
        try_from_slice_unchecked::<FeeDistribution>(&new_fee_distribution_info.data.borrow())?;
    new_fee_distribution.check_sum()?;

    lido.fee_distribution = *new_fee_distribution_info.key;

    lido.serialize(&mut *lido_info.data.borrow_mut())
        .map_err(|e| e.into())
}

// TODO(fynn): Change in favour of integrating the state to Lido's
pub fn process_add_validator(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();

    // Stake pool info
    let stake_pool_program_id_info = next_account_info(account_info_iter)?;
    let stake_pool_info = next_account_info(account_info_iter)?;
    let stake_pool_withdraw_authority_info = next_account_info(account_info_iter)?;
    let stake_pool_validator_list_info = next_account_info(account_info_iter)?;

    let stake_account_info = next_account_info(account_info_iter)?;
    let validator_lsol_account_info = next_account_info(account_info_iter)?;

    let lido_info = next_account_info(account_info_iter)?;
    let manager_info = next_account_info(account_info_iter)?;
    let fee_distribution_info = next_account_info(account_info_iter)?;
    let validator_credit_accounts_info = next_account_info(account_info_iter)?;

    // Sys
    let clock_info = next_account_info(account_info_iter)?;
    let stake_history = next_account_info(account_info_iter)?;

    let mut lido = try_from_slice_unchecked::<Lido>(&lido_info.data.borrow())?;
    if &lido.stake_pool_account != stake_pool_info.key {
        msg!("Invalid stake pool");
        return Err(LidoError::InvalidStakePool.into());
    }

    if !manager_info.is_signer {
        msg!("Message needs to be signed by Lido's manager");
        return Err(LidoError::InvalidOwner.into());
    }
    if &lido.fee_distribution != fee_distribution_info.key {
        msg!("Invalid current fee distribution account");
        return Err(LidoError::InvalidFeeDistributionAccount.into());
    }
    let validator_st_sol_account =
        spl_token::state::Account::unpack_from_slice(&validator_lsol_account_info.data.borrow())?;
    if lido.lsol_mint_program != validator_st_sol_account.mint {
        msg!(
            "Validator account minter should be the same as Lido minter {}",
            lido.lsol_mint_program
        );
        return Err(LidoError::InvalidTokenMinter.into());
    }

    invoke_signed(
        &add_validator_to_pool(
            stake_pool_program_id_info.key,
            stake_pool_info.key,
            manager_info.key,
            stake_pool_withdraw_authority_info.key,
            stake_pool_validator_list_info.key,
            stake_account_info.key,
        )?,
        &[
            stake_pool_info.clone(),
            manager_info.clone(),
            stake_pool_withdraw_authority_info.clone(),
            stake_pool_validator_list_info.clone(),
            stake_account_info.clone(),
            clock_info.clone(),
            stake_history.clone(),
            stake_pool_program_id_info.clone(),
        ],
        &[&[
            &lido_info.key.to_bytes()[..32],
            STAKE_POOL_MANAGER,
            &[lido.sol_reserve_authority_bump_seed],
        ]],
    )?;

    let mut validator_credit_accounts = try_from_slice_unchecked::<ValidatorCreditAccounts>(
        &validator_credit_accounts_info.data.borrow(),
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
            address: *validator_lsol_account_info.key,
            amount: 0,
        });

    validator_credit_accounts
        .serialize(&mut *validator_credit_accounts_info.data.borrow_mut())
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
    let account_info_iter = &mut accounts.iter();

    // Stake pool info
    let stake_pool_program_id_info = next_account_info(account_info_iter)?;
    let stake_pool_info = next_account_info(account_info_iter)?;
    let stake_pool_withdraw_authority_info = next_account_info(account_info_iter)?;
    let stake_pool_validator_list_info = next_account_info(account_info_iter)?;

    let stake_account_info = next_account_info(account_info_iter)?;
    let validator_lsol_account_info = next_account_info(account_info_iter)?;

    let lido_info = next_account_info(account_info_iter)?;
    let stake_pool_token_holder_info = next_account_info(account_info_iter)?;

    let fee_distribution_info = next_account_info(account_info_iter)?;
    let validator_credit_accounts_info = next_account_info(account_info_iter)?;
    unimplemented!()
}

pub fn process_distribute_fees(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();

    let lido_info = next_account_info(account_info_iter)?;
    let validator_credit_accounts_info = next_account_info(account_info_iter)?;
    let fee_distribution_info = next_account_info(account_info_iter)?;
    let token_holder_stake_pool_info = next_account_info(account_info_iter)?;
    let fee_manager_info = next_account_info(account_info_iter)?;
    let token_program_info = next_account_info(account_info_iter)?;
    let mint_program = next_account_info(account_info_iter)?;
    let reserve_authority_info = next_account_info(account_info_iter)?;

    // Recipients
    let insurance_info = next_account_info(account_info_iter)?;
    let treasury_info = next_account_info(account_info_iter)?;
    let manager_info = next_account_info(account_info_iter)?;

    let stake_pool_info = next_account_info(account_info_iter)?;
    let stake_pool_token_program_info = next_account_info(account_info_iter)?;
    let stake_pool_validator_list_info = next_account_info(account_info_iter)?;
    let stake_pool_fee_account_info = next_account_info(account_info_iter)?;
    let stake_pool_manager_fee_account_info = next_account_info(account_info_iter)?;

    let lido = try_from_slice_unchecked::<Lido>(&lido_info.data.borrow())?;

    if &lido.stake_pool_account != stake_pool_info.key {
        msg!("Invalid stake pool");
        return Err(LidoError::InvalidStakePool.into());
    }
    if &lido.validator_credit_accounts != validator_credit_accounts_info.key {
        msg!("Wrong validator credit accounts");
        return Err(LidoError::InvalidValidatorCreditAccount.into());
    }
    if &lido.fee_distribution != fee_distribution_info.key {
        msg!("Wrong fee distribution");
        return Err(LidoError::InvalidFeeDistributionAccount.into());
    }

    let stake_pool = StakePool::try_from_slice(&stake_pool_info.data.borrow())?;
    if &stake_pool.validator_list != stake_pool_validator_list_info.key {
        msg!("Invalid validators list from StakePool");
        return Err(StakePoolError::InvalidValidatorStakeList.into());
    }
    if &stake_pool.manager_fee_account != stake_pool_fee_account_info.key {
        msg!("Invalid fee account from StakePool");
        return Err(StakePoolError::InvalidFeeAccount.into());
    }
    let stake_pool_fee_account =
        spl_token::state::Account::unpack_from_slice(&stake_pool_fee_account_info.data.borrow())?;

    let validator_list =
        try_from_slice_unchecked::<ValidatorList>(&stake_pool_validator_list_info.data.borrow())?;

    let fee_distribution =
        try_from_slice_unchecked::<FeeDistribution>(&fee_distribution_info.data.borrow())?;

    let token_shares = fee_distribution.calculate_token_amounts(
        stake_pool_fee_account.amount,
        validator_list.validators.len() as u32,
    )?;

    let mut validator_credit_accounts = try_from_slice_unchecked::<ValidatorCreditAccounts>(
        &validator_credit_accounts_info.data.borrow(),
    )?;

    // Send all tokens to Lido token holder
    transfer_to(
        lido_info.key,
        stake_pool_token_program_info.clone(),
        stake_pool_manager_fee_account_info.clone(),
        token_holder_stake_pool_info.clone(),
        fee_manager_info.clone(),
        FEE_MANAGER_AUTHORITY,
        lido.fee_manager_bump_seed,
        stake_pool_fee_account.amount,
    )?;

    // Mint tokens for insurance
    token_mint_to(
        lido_info.key,
        token_program_info.clone(),
        mint_program.clone(),
        insurance_info.clone(),
        reserve_authority_info.clone(),
        RESERVE_AUTHORITY,
        lido.sol_reserve_authority_bump_seed,
        token_shares.insurance_amount,
    )?;
    // Mint tokens for treasury
    token_mint_to(
        lido_info.key,
        token_program_info.clone(),
        mint_program.clone(),
        treasury_info.clone(),
        reserve_authority_info.clone(),
        RESERVE_AUTHORITY,
        lido.sol_reserve_authority_bump_seed,
        token_shares.treasury_amount,
    )?;
    // Mint tokens for manager
    token_mint_to(
        lido_info.key,
        token_program_info.clone(),
        mint_program.clone(),
        manager_info.clone(),
        reserve_authority_info.clone(),
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
        .serialize(&mut *validator_credit_accounts_info.data.borrow_mut())
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
