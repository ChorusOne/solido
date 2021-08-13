// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

#![cfg(feature = "test-bpf")]

//! This module tests our assumptions about how Solana works.
//!
//! In some places the Solana documentation is absent or incomplete, so we test
//! the implementation to see how Solana actually works.

use solana_program_test::tokio;
use solana_sdk::signature::Signer;

use lido::token::Lamports;

use crate::context::Context;

/// Test that `solana_program::stake::instruction::merge` is symmetric.
///
/// <https://docs.solana.com/staking/stake-accounts#merging-stake-accounts>
/// suggests that merge may not be symmetric, it says:
///
/// > A merge is possible between two stakes in the following states with no
/// > additional conditions:
/// > * an inactive stake into an activating stake during its activation epoch.
///
/// But the reverse case of merging activating stake into an inactive stake
/// account is not mentioned. In this test, we confirm that the both cases are
/// allowed, and that merging stake is in fact symmetric.
#[tokio::test]
async fn test_stake_merge_is_symmetric() {
    let amount = Lamports(1_000_000_000);

    let mut context = Context::new_with_maintainer_and_validator().await;
    let vote_account = context.validator.as_ref().unwrap().vote_account.clone();
    let authority = context.deterministic_keypair.new_keypair();

    // Case 1: merge inactive into activating stake.
    let activating = context
        .create_stake_account(amount, authority.pubkey())
        .await;
    let inactive = context
        .create_stake_account(amount, authority.pubkey())
        .await;
    context
        .delegate_stake_account(activating, vote_account, &authority)
        .await;
    context
        .merge_stake_accounts(inactive, activating, &authority)
        .await;

    // Case 2: merge activating into inactive stake.
    let activating = context
        .create_stake_account(amount, authority.pubkey())
        .await;
    let inactive = context
        .create_stake_account(amount, authority.pubkey())
        .await;
    context
        .delegate_stake_account(activating, vote_account, &authority)
        .await;
    context
        .merge_stake_accounts(activating, inactive, &authority)
        .await;

    // If we get here, then both merges worked.
}

enum StakeMode {
    Inactive,
    Active,
    Deactivating,
}

/// Set up a stake account, possibly activate it, wait for it to be active,
/// possibly deactivate it, and then measure how many rewards we got.
///
/// We have to spin up the entire test context for this, to ensure that the
/// stake being active or deactivating is the only difference. The staking
/// rewards depend on the epoch number, and they appear to also depend on the
/// stake history, so we can’t just measure in two consecutive epochs, because
/// the rewards would differ.
async fn measure_staking_rewards(mode: StakeMode) -> Lamports {
    let amount = Lamports(1_000_000_000);

    let mut context = Context::new_with_maintainer_and_validator().await;
    let vote_account = context.validator.as_ref().unwrap().vote_account.clone();
    let authority = context.deterministic_keypair.new_keypair();

    let epoch_schedule = context.context.genesis_config().epoch_schedule;
    let slots_per_epoch = epoch_schedule.slots_per_epoch;
    let start_slot = epoch_schedule.first_normal_slot;

    context.context.warp_to_slot(start_slot).unwrap();

    // Create a stake account and delegate it to the vote account, which is a
    // 100% commission vote account, so all rewards go to the vote account.
    let stake_account = context
        .create_stake_account(amount, authority.pubkey())
        .await;

    match mode {
        StakeMode::Inactive => { /* Don't activate the stake if we want inactive stake. */ }
        StakeMode::Active | StakeMode::Deactivating => {
            context
                .delegate_stake_account(stake_account, vote_account, &authority)
                .await;
        }
    }

    // Move ahead one epoch so the stake becomes active.
    context
        .context
        .warp_to_slot(start_slot + 1 * slots_per_epoch)
        .unwrap();

    let balance_t0 = context.get_sol_balance(vote_account).await;

    // Deactivate the stake if needed.
    match mode {
        StakeMode::Inactive | StakeMode::Active => {}
        StakeMode::Deactivating => {
            context
                .deactivate_stake_account(stake_account, &authority)
                .await
        }
    }

    // Vote, and then move one more epoch, so we get the validation rewards.
    context
        .context
        .increment_vote_account_credits(&vote_account, 1);
    context
        .context
        .warp_to_slot(start_slot + 2 * slots_per_epoch)
        .unwrap();

    let balance_t1 = context.get_sol_balance(vote_account).await;

    (balance_t1 - balance_t0).unwrap()
}

/// Confirm that deactivating stake still earns rewards in that epoch.
#[tokio::test]
async fn test_deactivating_stake_earns_rewards() {
    let rewards_inactive = measure_staking_rewards(StakeMode::Inactive).await;
    let rewards_active = measure_staking_rewards(StakeMode::Active).await;
    let rewards_deactivating = measure_staking_rewards(StakeMode::Deactivating).await;

    // When stake is deactivating, the rewards are a few lamports less, because
    // the deactivation transaction itself costs a transaction fee, which is
    // burned, and this affects the rewards. See also
    // https://github.com/solana-labs/solana/issues/18894. Two Lamports out of
    // 1.2k SOL is a negligible difference, so we'll assume that deactivation
    // does not prevent rewards.
    assert_eq!(rewards_inactive, Lamports(0));
    assert_eq!(rewards_active, Lamports(1_244_922_235_900));
    assert_eq!(rewards_deactivating, Lamports(1_244_922_235_898));
}
