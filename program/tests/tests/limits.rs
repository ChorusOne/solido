// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

//! This test confirms the limits of our system.
//!
//! If the tests in here start to fail, we probably need to update the test
//! expectations; there is no "right" answer, but we would like to know what
//! how many accounts Solido can handle.

use testlib::solido_context::Context;

use lido::state::StakeDeposit;
use lido::token::Lamports;

use solana_program::pubkey::Pubkey;
use solana_program_test::tokio;

/// Test how many stake accounts per validator we can support.
///
/// This test is mostly for informational purposes, if it fails, adjust the
/// expected `max_accounts` below. We do need at least ~3 stake accounts per
/// validator (one activating, one active but unmergeable due to a Solana bug,
/// and one active and mergeable).
#[tokio::test]
async fn test_update_stake_account_balance_max_accounts() {
    let mut context = Context::new_with_maintainer().await;
    let validator = context.add_validator().await;

    // The maximum number of stake accounts per validator that we can support,
    // before UpdateStakeAccountBalance fails.
    let max_accounts = 9;

    for i in 0..=max_accounts {
        let amount = Lamports(2_000_000_000);
        context.deposit(amount).await;
        let stake_account = context
            .stake_deposit(validator.vote_account, StakeDeposit::Append, amount)
            .await;

        // Put some additional SOL in the stake account, so `UpdateStakeAccountBalance`
        // has something to withdraw. This consumes more compute units than a
        // no-op update, so we actually test the worst case.
        context.fund(stake_account, Lamports(100_000)).await;

        let result = context
            .try_update_stake_account_balance(validator.vote_account)
            .await;

        if i < max_accounts {
            assert!(
                result.is_ok(),
                "WithdrawInactiveStake should succeed with {} out of max {} stake accounts.",
                i + 1,
                max_accounts,
            );
        } else {
            // One more account should fail. At the time of writing, it fails
            // because it runs into the compute unit limit.
            assert!(result.is_err());
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_max_validators_maintainers() {
    let mut context = Context::new_with_maintainer().await;

    // The maximum number of validators that we can support, before Deposit or
    // StakeDeposit fails.
    let max_validators: u32 = 6_700;

    let mut first_validator_vote_account = Pubkey::default();
    for i in 0..max_validators {
        // Initially expect every validator to be a maintainer as well, so let's
        // add a maintainer for every validator. We set this to be the context's
        // maintainer that is used to sign `stake_deposit`.
        let maintainer = context.add_maintainer().await;
        context.maintainer = Some(maintainer);

        let validator = context.add_validator().await;
        if i == 0 {
            first_validator_vote_account = validator.vote_account;
        }
        // test with step 100 to reduce waiting
        if (i + 1) % 100 == 0 {
            context
                .memo(&format!("Testing heavy load {}.", i + 1))
                .await;

            let amount = Lamports(2_000_000_000);
            context.deposit(amount).await;
            context
                .stake_deposit(validator.vote_account, StakeDeposit::Append, amount)
                .await;
            context
                .unstake(validator.vote_account, Lamports(1_000_000_000))
                .await;
            // If we get here, then none of the transactions failed.
        }
    }

    // remove from the beginning of a list to test worst case
    context
        .deactivate_validator(first_validator_vote_account)
        .await;
    context
        .try_remove_validator(first_validator_vote_account)
        .await
        .expect("Could not remove first validator");
}
