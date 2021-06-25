#![cfg(feature = "test-bpf")]

use crate::context::Context;

use lido::token::Lamports;
use solana_program_test::tokio;

pub const TEST_DEPOSIT_AMOUNT: Lamports = Lamports(1000);

#[tokio::test]
async fn test_successful_deposit() {
    let mut context = Context::new_with_maintainer_and_validator().await;

    let recipient = context.deposit(TEST_DEPOSIT_AMOUNT).await;

    let reserve_balance = context.get_sol_balance(context.reserve_address).await;
    let rent = context.get_rent().await;
    assert_eq!(
        Some(reserve_balance),
        TEST_DEPOSIT_AMOUNT + Lamports(rent.minimum_balance(0))
    );

    // In general, the received stSOL need not be equal to the deposited SOL,
    // but initially, the exchange rate is 1, so this holds.
    let st_sol_balance = context.get_st_sol_balance(recipient).await;
    assert_eq!(st_sol_balance.0, TEST_DEPOSIT_AMOUNT.0);
}
