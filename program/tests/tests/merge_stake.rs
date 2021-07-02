#![cfg(feature = "test-bpf")]

use crate::assert_solido_error;
use crate::context::Context;
use lido::{error::LidoError, state::Validator, token::Lamports};
use solana_program_test::tokio;
use solana_sdk::signer::Signer;

// TODO(#226): We test only merging inactive stake accounts, test also other combinations.
#[tokio::test]
async fn test_successful_merge_stake_beginning() {
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

#[tokio::test]
async fn test_successful_merge_stake_end() {
    let (mut context, stake_account_pubkeys) = Context::new_with_two_stake_accounts().await;
    let solido_before = context.get_solido().await;
    let validator_vote_account = context.validator.as_ref().unwrap().vote_account;
    context.merge_stake(validator_vote_account, 1, 0).await;

    let account = context.try_get_account(stake_account_pubkeys[1]).await;
    assert!(account.is_none());
    let (meta, stake) = context.get_stake_state(stake_account_pubkeys[0]).await;
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
        validator_after.entry.stake_accounts_seed_end,
        validator_before.entry.stake_accounts_seed_end - 1,
    );
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
        .stake_deposit(validator.vote_account, Lamports(10_000_000_000))
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
    context.merge_stake(validator_vote_account, 1, 0).await;
    let reserve_balance_after = context.get_sol_balance(context.reserve_address).await;
    assert_eq!(
        (reserve_balance_before + Lamports(1_000_000_000)).unwrap(),
        reserve_balance_after
    );
}
