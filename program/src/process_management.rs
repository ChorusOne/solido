use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, msg, program_pack::Pack, pubkey::Pubkey,
};
use spl_stake_pool::{error::StakePoolError, state::StakePool};

use crate::{
    error::LidoError,
    instruction::{
        AddMaintainerInfo, AddValidatorInfo, ChangeFeeSpecInfo, ClaimValidatorFeeInfo,
        DistributeFeesInfo, RemoveMaintainerInfo, RemoveValidatorInfo,
    },
    logic::{deserialize_lido, token_mint_to, transfer_to},
    state::{distribute_fees, FeeDistribution, Lido, Validator},
    token::{StLamports, StakePoolTokenLamports},
    FEE_MANAGER_AUTHORITY, RESERVE_AUTHORITY,
};

pub fn process_change_fee_spec(
    program_id: &Pubkey,
    new_fee_distribution: FeeDistribution,
    accounts_raw: &[AccountInfo],
) -> ProgramResult {
    let accounts = ChangeFeeSpecInfo::try_from_slice(accounts_raw)?;
    let mut lido = deserialize_lido(program_id, accounts.lido)?;
    lido.check_manager(accounts.manager)?;

    Lido::check_valid_minter_program(&lido.st_sol_mint, accounts.treasury_account)?;
    Lido::check_valid_minter_program(&lido.st_sol_mint, accounts.developer_account)?;

    lido.fee_distribution = new_fee_distribution;
    lido.fee_recipients.treasury_account = *accounts.treasury_account.key;
    lido.fee_recipients.developer_account = *accounts.developer_account.key;

    lido.serialize(&mut *accounts.lido.data.borrow_mut())
        .map_err(|e| e.into())
}

pub fn process_add_validator(program_id: &Pubkey, accounts_raw: &[AccountInfo]) -> ProgramResult {
    let accounts = AddValidatorInfo::try_from_slice(accounts_raw)?;
    let mut lido = deserialize_lido(program_id, accounts.lido)?;
    lido.check_manager(accounts.manager)?;

    let validator_fee_st_sol_account = spl_token::state::Account::unpack_from_slice(
        &accounts.validator_fee_st_sol_account.data.borrow(),
    )?;
    if lido.st_sol_mint != validator_fee_st_sol_account.mint {
        msg!("Validator fee account minter should be the same as Lido minter.");
        msg!(
            "Expected {}, got {}.",
            lido.st_sol_mint,
            validator_fee_st_sol_account.mint
        );
        return Err(LidoError::InvalidTokenMinter.into());
    }

    lido.validators.add(
        *accounts.validator_vote_account.key,
        Validator::new(*accounts.validator_fee_st_sol_account.key),
    )?;
    lido.serialize(&mut *accounts.lido.data.borrow_mut())
        .map_err(|err| err.into())
}

/// Removes a validator from the stake pool, notice that the validator might not
/// be immediately removed from the validators list in the stake pool after this
/// instruction is executed, this function requires the validator has no
/// unclaimed fees.
/// The validator stake account to be removed:
/// `accounts::stake_account_to_remove` should have exactly 1 Sol + rent for
/// holding a Stake account, this is checked in `remove_validator_from_pool` from
/// the Stake Pool.
pub fn process_remove_validator(
    program_id: &Pubkey,
    accounts_raw: &[AccountInfo],
) -> ProgramResult {
    let accounts = RemoveValidatorInfo::try_from_slice(accounts_raw)?;
    let mut lido = deserialize_lido(program_id, accounts.lido)?;
    lido.check_manager(accounts.manager)?;

    let removed_validator = lido
        .validators
        .remove(accounts.validator_vote_account_to_remove.key)?;

    if removed_validator.fee_credit != StLamports(0) {
        msg!("Validator still has tokens to claim. Reclaim tokens before removing the validator");
        return Err(LidoError::ValidatorHasUnclaimedCredit.into());
    }

    lido.serialize(&mut *accounts.lido.data.borrow_mut())
        .map_err(|err| err.into())
}

pub fn process_claim_validator_fee(
    program_id: &Pubkey,
    accounts_raw: &[AccountInfo],
) -> ProgramResult {
    let accounts = ClaimValidatorFeeInfo::try_from_slice(accounts_raw)?;
    let mut lido = deserialize_lido(program_id, accounts.lido)?;

    let pubkey_entry = lido
        .validators
        .entries
        .iter_mut()
        .find(|pe| &pe.entry.fee_address == accounts.validator_fee_st_sol_account.key)
        .ok_or(LidoError::InvalidValidatorCreditAccount)?;

    token_mint_to(
        accounts.lido.key,
        accounts.spl_token.clone(),
        accounts.st_sol_mint.clone(),
        accounts.validator_fee_st_sol_account.clone(),
        accounts.reserve_authority.clone(),
        RESERVE_AUTHORITY,
        lido.sol_reserve_authority_bump_seed,
        pubkey_entry.entry.fee_credit,
    )?;
    pubkey_entry.entry.fee_credit = StLamports(0);
    lido.serialize(&mut *accounts.lido.data.borrow_mut())
        .map_err(|err| err.into())
}

pub fn process_distribute_fees(program_id: &Pubkey, accounts_raw: &[AccountInfo]) -> ProgramResult {
    let accounts = DistributeFeesInfo::try_from_slice(accounts_raw)?;
    let mut lido = deserialize_lido(program_id, accounts.lido)?;
    lido.check_maintainer(accounts.maintainer)?;
    lido.check_stake_pool(accounts.stake_pool)?;

    let stake_pool = StakePool::try_from_slice(&accounts.stake_pool.data.borrow())?;
    if &stake_pool.manager_fee_account != accounts.stake_pool_fee_account.key {
        msg!("Invalid fee account from StakePool");
        return Err(StakePoolError::InvalidFeeAccount.into());
    }
    let stake_pool_fee_account = spl_token::state::Account::unpack_from_slice(
        &accounts.stake_pool_fee_account.data.borrow(),
    )?;

    let token_shares = distribute_fees(
        &lido.fee_distribution,
        lido.validators.len() as u64,
        StakePoolTokenLamports(stake_pool_fee_account.amount),
    )
    .ok_or(LidoError::CalculationFailure)?;

    // Send all tokens to Lido token holder
    transfer_to(
        accounts.lido.key,
        accounts.spl_token.clone(),
        accounts.stake_pool_fee_account.clone(),
        accounts.token_holder_stake_pool.clone(),
        accounts.stake_pool_manager_fee_account.clone(),
        FEE_MANAGER_AUTHORITY,
        lido.fee_manager_bump_seed,
        stake_pool_fee_account.amount,
    )?;

    // Mint tokens for treasury
    token_mint_to(
        accounts.lido.key,
        accounts.spl_token.clone(),
        accounts.st_sol_mint.clone(),
        accounts.treasury_account.clone(),
        accounts.reserve_authority.clone(),
        RESERVE_AUTHORITY,
        lido.sol_reserve_authority_bump_seed,
        token_shares.treasury_amount,
    )?;
    // Mint tokens for developer
    token_mint_to(
        accounts.lido.key,
        accounts.spl_token.clone(),
        accounts.st_sol_mint.clone(),
        accounts.developer_account.clone(),
        accounts.reserve_authority.clone(),
        RESERVE_AUTHORITY,
        lido.sol_reserve_authority_bump_seed,
        token_shares.developer_amount,
    )?;

    // Update validator list that can be claimed at a later time
    for pe in lido.validators.entries.iter_mut() {
        pe.entry.fee_credit = (pe.entry.fee_credit + token_shares.reward_per_validator)
            .ok_or(LidoError::CalculationFailure)?;
    }
    lido.serialize(&mut *accounts.lido.data.borrow_mut())
        .map_err(|err| err.into())
}

/// Adds a maintainer to the list of maintainers
pub fn process_add_maintainer(program_id: &Pubkey, accounts_raw: &[AccountInfo]) -> ProgramResult {
    let accounts = AddMaintainerInfo::try_from_slice(accounts_raw)?;
    let mut lido = deserialize_lido(program_id, accounts.lido)?;
    lido.check_manager(accounts.manager)?;

    lido.maintainers.add(*accounts.maintainer.key, ())?;
    lido.serialize(&mut *accounts.lido.data.borrow_mut())
        .map_err(|err| err.into())
}

/// Removes a maintainer from the list of maintainers
pub fn process_remove_maintainer(
    program_id: &Pubkey,
    accounts_raw: &[AccountInfo],
) -> ProgramResult {
    let accounts = RemoveMaintainerInfo::try_from_slice(accounts_raw)?;
    let mut lido = deserialize_lido(program_id, accounts.lido)?;
    lido.check_manager(accounts.manager)?;

    lido.maintainers.remove(accounts.maintainer.key)?;
    lido.serialize(&mut *accounts.lido.data.borrow_mut())
        .map_err(|err| err.into())
}

/// TODO(#186) Allow validator to change fee account
/// Called by the validator, changes the fee account which the validator
/// receives tokens
pub fn _process_change_validator_fee_account(
    _program_id: &Pubkey,
    _accounts: &[AccountInfo],
) -> ProgramResult {
    unimplemented!()
}
