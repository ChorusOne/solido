#![cfg(feature = "test-bpf")]

use crate::context::Context;

use lido::token::Lamports;
use solana_program_test::tokio;

pub const TEST_DEPOSIT_AMOUNT: Lamports = Lamports(100_000_000_000);
pub const TEST_STAKE_DEPOSIT_AMOUNT: Lamports = Lamports(10_000_000_000);

#[tokio::test]
async fn test_successful_stake_deposit() {
    let mut context = Context::new_with_maintainer().await;
    let validator = context.add_validator().await;

    // Sanity check before we start: the validator should have zero balance in zero stake accounts.
    let solido_before = context.get_solido().await;
    let validator_before = &solido_before.validators.entries[0].entry;
    assert_eq!(validator_before.stake_accounts_balance, Lamports(0));
    assert_eq!(validator_before.stake_accounts_seed_begin, 0);
    assert_eq!(validator_before.stake_accounts_seed_end, 0);

    // Now we make a deposit, and then delegate part of it.
    context.deposit(TEST_DEPOSIT_AMOUNT).await;

    let stake_account = context
        .stake_deposit(validator.vote_account, TEST_STAKE_DEPOSIT_AMOUNT)
        .await;

    // The amount that we staked, should now be in the stake account.
    assert_eq!(
        context.get_sol_balance(stake_account).await,
        TEST_STAKE_DEPOSIT_AMOUNT
    );

    // We should also have recorded in the Solido state that this validator now
    // has balance in a stake account.
    let solido_after = context.get_solido().await;

    let validator_after = &solido_after.validators.entries[0].entry;
    assert_eq!(
        validator_after.stake_accounts_balance,
        TEST_STAKE_DEPOSIT_AMOUNT
    );

    // This was also the first deposit, so that should have created one stake account.
    assert_eq!(validator_after.stake_accounts_seed_begin, 0);
    assert_eq!(validator_after.stake_accounts_seed_end, 1);
}

#[tokio::test]
// TODO(#187) Implement test for stake_exists_stake_deposit
async fn test_stake_exists_stake_deposit() {}
