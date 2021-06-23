use borsh::BorshSerialize;
use solana_program::program::invoke_signed;
use solana_program::{account_info::AccountInfo, entrypoint::ProgramResult, msg, pubkey::Pubkey};
use spl_stake_pool::stake_program;

use crate::token::Lamports;
use crate::DEPOSIT_AUTHORITY;
use crate::{
    error::LidoError,
    instruction::{
        AddMaintainerInfo, AddValidatorInfo, ChangeFeeSpecInfo, ClaimValidatorFeeInfo,
        DistributeFeesInfo, MergeStakeInfo, RemoveMaintainerInfo, RemoveValidatorInfo,
    },
    logic::{deserialize_lido, token_mint_to},
    state::{distribute_fees, FeeDistribution, Validator},
    token::StLamports,
    RESERVE_AUTHORITY,
};

pub fn process_change_fee_spec(
    program_id: &Pubkey,
    new_fee_distribution: FeeDistribution,
    accounts_raw: &[AccountInfo],
) -> ProgramResult {
    let accounts = ChangeFeeSpecInfo::try_from_slice(accounts_raw)?;
    let mut lido = deserialize_lido(program_id, accounts.lido)?;
    lido.check_manager(accounts.manager)?;

    lido.check_is_st_sol_account(&accounts.treasury_account)?;
    lido.check_is_st_sol_account(&accounts.developer_account)?;

    lido.fee_distribution = new_fee_distribution;
    lido.fee_recipients.treasury_account = *accounts.treasury_account.key;
    lido.fee_recipients.developer_account = *accounts.developer_account.key;

    lido.save(accounts.lido)
}

pub fn process_add_validator(program_id: &Pubkey, accounts_raw: &[AccountInfo]) -> ProgramResult {
    let accounts = AddValidatorInfo::try_from_slice(accounts_raw)?;
    let mut lido = deserialize_lido(program_id, accounts.lido)?;
    lido.check_manager(accounts.manager)?;
    lido.check_is_st_sol_account(&accounts.validator_fee_st_sol_account)?;

    lido.validators.add(
        *accounts.validator_vote_account.key,
        Validator::new(*accounts.validator_fee_st_sol_account.key),
    )?;

    lido.save(accounts.lido)
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

    lido.save(accounts.lido)
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

    lido.save(accounts.lido)
}

pub fn process_distribute_fees(program_id: &Pubkey, accounts_raw: &[AccountInfo]) -> ProgramResult {
    let accounts = DistributeFeesInfo::try_from_slice(accounts_raw)?;
    let mut lido = deserialize_lido(program_id, accounts.lido)?;
    lido.check_maintainer(accounts.maintainer)?;

    let token_shares = distribute_fees(
        &lido.fee_distribution,
        lido.validators.len() as u64,
        // TODO(#178): Compute the rewards, and then distribute fees.
        Lamports(0),
    )
    .ok_or(LidoError::CalculationFailure)?;

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

    lido.save(accounts.lido)
}

/// Adds a maintainer to the list of maintainers
pub fn process_add_maintainer(program_id: &Pubkey, accounts_raw: &[AccountInfo]) -> ProgramResult {
    let accounts = AddMaintainerInfo::try_from_slice(accounts_raw)?;
    let mut lido = deserialize_lido(program_id, accounts.lido)?;
    lido.check_manager(accounts.manager)?;

    lido.maintainers.add(*accounts.maintainer.key, ())?;

    lido.save(accounts.lido)
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

    lido.save(accounts.lido)
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

/// Merge two stake accounts, they should both be fully active. It will merge
/// `from_stake` to `to_stake`. Bumps `begin` to `begin+1` if the operation was
/// successful.
pub fn process_merge_stake(program_id: &Pubkey, accounts_raw: &[AccountInfo]) -> ProgramResult {
    let accounts = MergeStakeInfo::try_from_slice(accounts_raw)?;
    let mut lido = deserialize_lido(program_id, accounts.lido)?;
    // Get validator.
    let mut validator = lido
        .validators
        .get_mut(accounts.validator_vote_account.key)?;
    // Check that there are at least two accounts to merge
    if validator.entry.stake_accounts_seed_begin == validator.entry.stake_accounts_seed_end {
        msg!("Attempting to merge accounts in a validator that has a single stake account.");
        return Err(LidoError::InvalidStakeAccount.into());
    }
    // Recalculate the `from_stake`.
    let (from_stake, _) = Validator::find_stake_account_address(
        program_id,
        accounts.lido.key,
        accounts.validator_vote_account.key,
        validator.entry.stake_accounts_seed_begin,
    );
    // Compare with the stake passed in `accounts`.
    if &from_stake != accounts.from_stake.key {
        msg!(
            "Calculated from_stake {} for seed {} is different from received {}.",
            from_stake,
            validator.entry.stake_accounts_seed_begin,
            accounts.from_stake.key
        );
        return Err(LidoError::InvalidStakeAccount.into());
    }
    let (to_stake, _) = Validator::find_stake_account_address(
        program_id,
        accounts.lido.key,
        accounts.validator_vote_account.key,
        validator.entry.stake_accounts_seed_begin + 1,
    );
    if &to_stake != accounts.to_stake.key {
        msg!(
            "Calculated to_stake {} for seed {} is different from received {}.",
            to_stake,
            validator.entry.stake_accounts_seed_end,
            accounts.to_stake.key
        );
        return Err(LidoError::InvalidStakeAccount.into());
    }
    // Merge `from_stake` to `to_stake`, at the end of the instruction,
    // `from_stake` ceases to exist.
    let merge_ix = stake_program::merge(&to_stake, &from_stake, &accounts.deposit_authority.key);
    invoke_signed(
        &merge_ix,
        &[
            accounts.from_stake.clone(),
            accounts.to_stake.clone(),
            accounts.sysvar_clock.clone(),
            accounts.stake_history.clone(),
            accounts.deposit_authority.clone(),
            accounts.stake_program.clone(),
        ],
        &[&[
            &accounts.lido.key.to_bytes(),
            DEPOSIT_AUTHORITY,
            &[lido.deposit_authority_bump_seed],
        ]],
    )?;
    // Bump the validator's stake_accounts_seed_begin.
    validator.entry.stake_accounts_seed_begin += 1;
    lido.serialize(&mut *accounts.lido.data.borrow_mut())?;
    Ok(())
}
