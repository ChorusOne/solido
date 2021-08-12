// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use solana_program::program::invoke_signed;
use solana_program::rent::Rent;
use solana_program::sysvar::Sysvar;
use solana_program::{account_info::AccountInfo, entrypoint::ProgramResult, msg, pubkey::Pubkey};

use crate::logic::check_rent_exempt;
use crate::vote_state::PartialVoteState;
use crate::{
    error::LidoError,
    instruction::{
        AddMaintainerInfo, AddValidatorInfo, ChangeRewardDistributionInfo, ClaimValidatorFeeInfo,
        MergeStakeInfo, RemoveMaintainerInfo, RemoveValidatorInfo,
    },
    logic::{deserialize_lido, mint_st_sol_to},
    state::{RewardDistribution, Validator},
    token::StLamports,
    STAKE_AUTHORITY,
};

pub fn process_change_reward_distribution(
    program_id: &Pubkey,
    new_reward_distribution: RewardDistribution,
    accounts_raw: &[AccountInfo],
) -> ProgramResult {
    let accounts = ChangeRewardDistributionInfo::try_from_slice(accounts_raw)?;
    let mut lido = deserialize_lido(program_id, accounts.lido)?;
    lido.check_manager(accounts.manager)?;

    lido.check_is_st_sol_account(accounts.treasury_account)?;
    lido.check_is_st_sol_account(accounts.developer_account)?;

    lido.reward_distribution = new_reward_distribution;
    lido.fee_recipients.treasury_account = *accounts.treasury_account.key;
    lido.fee_recipients.developer_account = *accounts.developer_account.key;

    lido.save(accounts.lido)
}

pub fn process_add_validator(program_id: &Pubkey, accounts_raw: &[AccountInfo]) -> ProgramResult {
    let accounts = AddValidatorInfo::try_from_slice(accounts_raw)?;
    let mut lido = deserialize_lido(program_id, accounts.lido)?;
    let rent = &Rent::from_account_info(accounts.sysvar_rent)?;
    lido.check_manager(accounts.manager)?;
    lido.check_is_st_sol_account(accounts.validator_fee_st_sol_account)?;

    check_rent_exempt(
        rent,
        accounts.validator_vote_account,
        "Validator vote account",
    )?;
    // Deserialize also checks if the vote account is a valid Solido vote
    // account: The withdraw authority should be set to the program_id, and it
    // should have 100% commission.
    let _partial_vote_state = PartialVoteState::deserialize(
        program_id,
        accounts.lido.key,
        &accounts.validator_vote_account.data.borrow(),
    )?;

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

    let amount_claimed = pubkey_entry.entry.fee_credit;
    pubkey_entry.entry.fee_credit = StLamports(0);

    mint_st_sol_to(
        &lido,
        accounts.lido.key,
        accounts.spl_token,
        accounts.st_sol_mint,
        accounts.mint_authority,
        accounts.validator_fee_st_sol_account,
        amount_claimed,
    )?;
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

/// Merge two stake accounts from the beginning of the validator's stake
/// accounts list.
/// This function can be called by anybody.
/// After this function, the validator's `stake_accounts_seed_begin` ceases to
/// exist and is merged with the stake defined by `stake_accounts_seed_begin +
/// 1`, and `stake_accounts_seed_begin` is incremented by one.
/// All fully active stake accounts precede the activating stake accounts.
pub fn process_merge_stake(program_id: &Pubkey, accounts_raw: &[AccountInfo]) -> ProgramResult {
    let accounts = MergeStakeInfo::try_from_slice(accounts_raw)?;
    let mut lido = deserialize_lido(program_id, accounts.lido)?;

    let mut validator = lido
        .validators
        .get_mut(accounts.validator_vote_account.key)?;
    let from_seed = validator.entry.stake_accounts_seed_begin;
    let to_seed = validator.entry.stake_accounts_seed_begin + 1;

    // Check that there are at least two accounts to merge
    if to_seed >= validator.entry.stake_accounts_seed_end {
        msg!("Attempting to merge accounts in a validator that has fewer than two stake accounts.");
        return Err(LidoError::InvalidStakeAccount.into());
    }

    // Recalculate the `from_stake`.
    let (from_stake_addr, _) = Validator::find_stake_account_address(
        program_id,
        accounts.lido.key,
        accounts.validator_vote_account.key,
        from_seed,
    );
    // Compare with the stake passed in `accounts`.
    if &from_stake_addr != accounts.from_stake.key {
        msg!(
            "Calculated from_stake {} for seed {} is different from received {}.",
            from_stake_addr,
            from_seed,
            accounts.from_stake.key
        );
        return Err(LidoError::InvalidStakeAccount.into());
    }
    let (to_stake_addr, _) = Validator::find_stake_account_address(
        program_id,
        accounts.lido.key,
        accounts.validator_vote_account.key,
        to_seed,
    );
    if &to_stake_addr != accounts.to_stake.key {
        msg!(
            "Calculated to_stake {} for seed {} is different from received {}.",
            to_stake_addr,
            to_seed,
            accounts.to_stake.key
        );
        return Err(LidoError::InvalidStakeAccount.into());
    }
    validator.entry.stake_accounts_seed_begin += 1;
    // Merge `from_stake_addr` to `to_stake_addr`, at the end of the
    // instruction, `from_stake_addr` ceases to exist.
    let merge_instructions = solana_program::stake::instruction::merge(
        &to_stake_addr,
        &from_stake_addr,
        accounts.stake_authority.key,
    );

    // For some reason, `merge` returns a `Vec`, but when we look at the
    // implementation, we can see that it always returns a single instruction.
    assert_eq!(merge_instructions.len(), 1);
    let merge_instruction = &merge_instructions[0];

    invoke_signed(
        merge_instruction,
        &[
            accounts.from_stake.clone(),
            accounts.to_stake.clone(),
            accounts.sysvar_clock.clone(),
            accounts.stake_history.clone(),
            accounts.stake_authority.clone(),
            accounts.stake_program.clone(),
        ],
        &[&[
            &accounts.lido.key.to_bytes(),
            STAKE_AUTHORITY,
            &[lido.stake_authority_bump_seed],
        ]],
    )?;

    lido.save(accounts.lido)
}
