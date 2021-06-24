#![cfg(feature = "test-bpf")]

use crate::context::Context;

use lido::token::Lamports;
use solana_program_test::tokio;

const NUMBER_VALIDATORS: u64 = 4;
const TEST_DEPOSIT_AMOUNT: Lamports = Lamports(100_000_000_000);
const EXTRA_STAKE_AMOUNT: Lamports = Lamports(50_000_000_000);

#[tokio::test]
async fn test_successful_fee_distribution() {
    let mut context = Context::new_with_maintainer().await;

    let mut validators = Vec::new();
    for _ in 0..NUMBER_VALIDATORS {
        validators.push(context.add_validator().await);
    }

    context.deposit(TEST_DEPOSIT_AMOUNT).await;

    let stake_account = context
        .stake_deposit(validators[0].vote_account, TEST_DEPOSIT_AMOUNT)
        .await;

    // Make `EXTRA_STAKE_AMOUNT` appear in the stake account, to simulate
    // validation rewards being paid out.
    // TODO(#207): this may not be the right way to simulate rewards.
    context.fund(stake_account, EXTRA_STAKE_AMOUNT).await;

    // TODO(#178): Restore the remainder of this test, once we implement validator
    // balance updates, and therefore fee distribution.
}
