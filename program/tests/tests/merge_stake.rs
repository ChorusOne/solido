#![cfg(feature = "test-bpf")]

use crate::assert_solido_error;
use crate::context::{Context, StakeDeposit};
use lido::{error::LidoError, stake_account::StakeAccount, state::Validator, token::Lamports};
use solana_program::pubkey::Pubkey;
use solana_program_test::tokio;
use solana_sdk::signer::Signer;

// TODO(#226): We test only merging inactive stake accounts, test also other combinations.
#[tokio::test]
async fn test_successful_merge_activating_stake() {
    let (mut context, stake_account_pubkeys) = Context::new_with_two_stake_accounts().await;
    let solido_before = context.get_solido().await;
    let validator_vote_account = context.validator.as_ref().unwrap().vote_account;
    context.merge_stake(validator_vote_account, 0, 1).await;

    let account = context.try_get_account(stake_account_pubkeys[0]).await;
    assert!(account.is_none());
    let (meta, stake) = context.get_stake_state(stake_account_pubkeys[1]).await;
    let sum = 20_000_000_000 - meta.rent_exempt_reserve;
    assert_eq!(
        stake.delegation.stake, sum,
        "Delegated stake should be {}, it is {} instead.",
        sum, stake.delegation.stake
    );

    let solido_after = context.get_solido().await;
    assert_eq!(
        solido_after
            .validators
            .get(&validator_vote_account)
            .unwrap()
            .entry
            .stake_accounts_balance,
        Lamports(20_000_000_000)
    );

    let validator_before = solido_before
        .validators
        .get(&validator_vote_account)
        .unwrap();
    let validator_after = solido_after
        .validators
        .get(&validator_vote_account)
        .unwrap();
    assert_eq!(
        validator_after.entry.stake_accounts_seed_begin,
        validator_before.entry.stake_accounts_seed_begin + 1,
    );
}

fn advance_epoch(context: &mut Context, current_slot: &mut u64) {
    let epoch_schedule = context.context.genesis_config().epoch_schedule;
    let slots_per_epoch = epoch_schedule.slots_per_epoch;
    *current_slot = *current_slot + slots_per_epoch;
    context.context.warp_to_slot(*current_slot).unwrap();
}

async fn get_stake_account_from_seed(
    context: &mut Context,
    validator_vote_account: &Pubkey,
    seed: u64,
) -> StakeAccount {
    let (stake_address, _) = Validator::find_stake_account_address(
        &crate::context::id(),
        &context.solido.pubkey(),
        &validator_vote_account,
        seed,
    );

    let clock = context.get_clock().await;
    let stake_history = context.get_stake_history().await;
    let stake_balance = context.get_sol_balance(stake_address).await;
    let (_, stake) = context.get_stake_state(stake_address).await;
    StakeAccount::from_delegated_account(stake_balance, &stake, &clock, &stake_history, seed)
}
// Test merging active to activating: should fail.
// Test merging two activated stake accounts: should succeed.
#[tokio::test]
async fn test_merge_stake_combinations() {
    let stake_deposit_amount = Lamports(2_000_000_000); // 2 Sol
    let mut context = Context::new_with_maintainer_and_validator().await;

    let validator_vote_account = context.validator.as_ref().unwrap().vote_account;
    context.deposit(Lamports(100_000_000_000)).await;
    context
        .stake_deposit(
            validator_vote_account,
            StakeDeposit::Append,
            stake_deposit_amount,
        )
        .await;

    let mut current_slot = 0;
    // Skip ahead 1 epoch
    advance_epoch(&mut context, &mut current_slot);

    // Create an activating stake account.
    context
        .stake_deposit(
            validator_vote_account,
            StakeDeposit::Append,
            stake_deposit_amount,
        )
        .await;

    let active_stake_account =
        get_stake_account_from_seed(&mut context, &validator_vote_account, 0).await;

    let activating_stake_account =
        get_stake_account_from_seed(&mut context, &validator_vote_account, 1).await;

    assert_eq!(active_stake_account.is_active(), true);
    assert_eq!(activating_stake_account.is_activating(), true);
    let result = context.try_merge_stake(validator_vote_account, 0, 1).await;
    // Merging active to activating should fail.
    assert_solido_error!(result, LidoError::WrongStakeState);
    advance_epoch(&mut context, &mut current_slot);

    let now_active_stake_account =
        get_stake_account_from_seed(&mut context, &validator_vote_account, 1).await;
    assert_eq!(active_stake_account.is_active(), true);
    assert_eq!(now_active_stake_account.is_active(), true);
    // Merging two activated stake accounts should succeed.
    context.merge_stake(validator_vote_account, 0, 1).await;
}

#[tokio::test]
async fn test_merge_validator_with_zero_and_one_stake_account() {
    let mut context = Context::new_with_maintainer().await;
    let validator = context.add_validator().await;
    context.deposit(Lamports(10_000_000_000)).await;

    // Try to merge stake on a validator that has no stake accounts.
    let result = context.try_merge_stake(validator.vote_account, 0, 1).await;
    assert_solido_error!(result, LidoError::InvalidStakeAccount);

    context
        .stake_deposit(
            validator.vote_account,
            StakeDeposit::Append,
            Lamports(10_000_000_000),
        )
        .await;

    // Try to merge stake on a validator that has 1 stake account.
    let result = context.try_merge_stake(validator.vote_account, 0, 1).await;
    assert_solido_error!(result, LidoError::InvalidStakeAccount);
}

#[tokio::test]
async fn test_merge_with_donated_stake() {
    let (mut context, _stake_account_pubkeys) = Context::new_with_two_stake_accounts().await;
    let validator_vote_account = context.validator.as_ref().unwrap().vote_account;
    let (from_stake_account, _) = Validator::find_stake_account_address(
        &crate::context::id(),
        &context.solido.pubkey(),
        &validator_vote_account,
        0,
    );
    context
        .fund(from_stake_account, Lamports(1_000_000_000))
        .await;

    let reserve_balance_before = context.get_sol_balance(context.reserve_address).await;
    context.merge_stake(validator_vote_account, 0, 1).await;
    let reserve_balance_after = context.get_sol_balance(context.reserve_address).await;
    assert_eq!(
        (reserve_balance_before + Lamports(1_000_000_000)).unwrap(),
        reserve_balance_after
    );
}
