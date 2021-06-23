#![cfg(feature = "test-bpf")]

mod helpers;

use helpers::{get_token_balance, program_test, LidoAccounts};
use lido::token::{Lamports, StLamports};
use solana_program_test::{tokio, ProgramTestContext};
use solana_sdk::signature::Signer;

async fn setup() -> (ProgramTestContext, LidoAccounts) {
    let mut context = program_test().start_with_context().await;
    let mut lido_accounts = LidoAccounts::new();
    lido_accounts.initialize_lido(&mut context).await;

    (context, lido_accounts)
}
pub const TEST_DEPOSIT_AMOUNT: Lamports = Lamports(1000);

#[tokio::test]
async fn test_successful_deposit() {
    let (mut context, accounts) = setup().await;

    let recipient = accounts.deposit(&mut context, TEST_DEPOSIT_AMOUNT).await;

    let reserve_account = context
        .banks_client
        .get_account(accounts.reserve_authority)
        .await
        .unwrap()
        .unwrap();

    let rent = context.banks_client.get_rent().await.unwrap();
    assert_eq!(
        Some(Lamports(reserve_account.lamports)),
        TEST_DEPOSIT_AMOUNT + Lamports(rent.minimum_balance(0))
    );

    // In general, the received stSOL need not be equal to the deposited SOL,
    // but initially, the exchange rate is 1, so this holds.
    let balance =
        StLamports(get_token_balance(&mut context.banks_client, &recipient.pubkey()).await);
    assert_eq!(balance.0, TEST_DEPOSIT_AMOUNT.0);
}
