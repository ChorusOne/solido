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
