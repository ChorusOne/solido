use solana_program::program::invoke_signed;
use solana_program::{account_info::AccountInfo, entrypoint::ProgramResult, msg, pubkey::Pubkey};
use spl_stake_pool::stake_program;

use crate::{
    error::LidoError,
    instruction::{
        AddMaintainerInfo, AddValidatorInfo, ChangeRewardDistributionInfo, ClaimValidatorFeeInfo,
        MergeStakeInfo, RemoveMaintainerInfo, RemoveValidatorInfo,
    },
    logic::{deserialize_lido, mint_st_sol_to},
    state::{RewardDistribution, Validator},
    token::{Lamports, StLamports},
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

    lido.check_is_st_sol_account(&accounts.treasury_account)?;
    lido.check_is_st_sol_account(&accounts.developer_account)?;

    lido.reward_distribution = new_reward_distribution;
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

    let amount_claimed = pubkey_entry.entry.fee_credit;
    pubkey_entry.entry.fee_credit = StLamports(0);

    mint_st_sol_to(
        &lido,
        accounts.lido.key,
        accounts.spl_token,
        accounts.st_sol_mint,
        accounts.reserve_authority,
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

/// Merge two stake accounts.
/// This function can be called by anybody.
/// After this function, the validator's list of stake accounts contains no gaps,
/// and all fully active stake accounts precede the activating stake accounts.
///
/// `from_seed` can be the validator's `stake_accounts_seed_begin` and in that
/// case `to_seed` should be `stake_accounts_seed_begin + 1`, or `from_seed` can
/// be the validator's `stake_accounts_seed_end - 1` and in that case `to_seed`
/// should be `stake_accounts_seed_end - 2`.
/// Validator stakes should both be fully active or both inactive when merging
/// stakes from the beginning, or both activating when merging stakes from the
/// end.
pub fn process_merge_stake(
    program_id: &Pubkey,
    from_seed: u64,
    to_seed: u64,
    accounts_raw: &[AccountInfo],
) -> ProgramResult {
    let accounts = MergeStakeInfo::try_from_slice(accounts_raw)?;
    let mut lido = deserialize_lido(program_id, accounts.lido)?;
    // Get validator.
    let mut validator = lido
        .validators
        .get_mut(accounts.validator_vote_account.key)?;
    // Check that there are at least two accounts to merge
    if validator.entry.stake_accounts_seed_begin + 1 >= validator.entry.stake_accounts_seed_end {
        msg!("Attempting to merge accounts in a validator that has fewer than two stake accounts.");
        return Err(LidoError::InvalidStakeAccount.into());
    }

    // check if merging from the beginning.
    if from_seed == validator.entry.stake_accounts_seed_begin
        && to_seed == validator.entry.stake_accounts_seed_begin + 1
    {
        // The stake accounts we try to merge are at the beginning, so the begin
        // account will go away.
        validator.entry.stake_accounts_seed_begin += 1;
    } else if from_seed == validator.entry.stake_accounts_seed_end - 1
        && to_seed == validator.entry.stake_accounts_seed_end - 2
    {
        // The stake accounts we try to merge are at the end, so the account
        // with seed `end - 1` will go away.
        validator.entry.stake_accounts_seed_end -= 1;
    } else {
        // The accounts to merge are not at the beginning or the end, we refuse
        // to merge them, as it would create a hole in the list of stake
        // accounts.
        msg!(
            "Attempting to merge stakes defined by {} and {}. 
        Only stake that are in the boundary indexes can be merged. ({} and {}, or {} and {})",
            from_seed,
            to_seed,
            validator.entry.stake_accounts_seed_begin,
            validator.entry.stake_accounts_seed_begin + 1,
            validator.entry.stake_accounts_seed_end - 1,
            validator.entry.stake_accounts_seed_end - 2
        );
        return Err(LidoError::WrongStakeState.into());
    }

    // Recalculate the `from_stake`.
    let (from_stake, _) = Validator::find_stake_account_address(
        program_id,
        accounts.lido.key,
        accounts.validator_vote_account.key,
        from_seed,
    );
    // Compare with the stake passed in `accounts`.
    if &from_stake != accounts.from_stake.key {
        msg!(
            "Calculated from_stake {} for seed {} is different from received {}.",
            from_stake,
            from_seed,
            accounts.from_stake.key
        );
        return Err(LidoError::InvalidStakeAccount.into());
    }
    let (to_stake, _) = Validator::find_stake_account_address(
        program_id,
        accounts.lido.key,
        accounts.validator_vote_account.key,
        to_seed,
    );
    if &to_stake != accounts.to_stake.key {
        msg!(
            "Calculated to_stake {} for seed {} is different from received {}.",
            to_stake,
            to_seed,
            accounts.to_stake.key
        );
        return Err(LidoError::InvalidStakeAccount.into());
    }
    // Merge `from_stake` to `to_stake`, at the end of the instruction,
    // `from_stake` ceases to exist.
    let merge_ix = stake_program::merge(&to_stake, &from_stake, &accounts.stake_authority.key);
    let lamports_from_stake_before = accounts.from_stake.lamports();
    let lamports_to_stake_before = accounts.to_stake.lamports();
    let sum_stakes = (Lamports(lamports_from_stake_before) + Lamports(lamports_to_stake_before))
        .expect("Summing lamports shouldn't overflow.");
    invoke_signed(
        &merge_ix,
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
    if Lamports(accounts.to_stake.lamports()) != sum_stakes {
        msg!("Something went wrong when merging the stakes, total amount in stake {} should be from_stake's Lamports ({}) + to_stake's Lamports ({}) = {}, is {} instead", accounts.to_stake.key, Lamports(lamports_from_stake_before), Lamports(lamports_to_stake_before), sum_stakes, Lamports(accounts.to_stake.lamports()));
        return Err(LidoError::CalculationFailure.into());
    }
    lido.save(accounts.lido)
}
