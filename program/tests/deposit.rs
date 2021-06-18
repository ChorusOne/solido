#![cfg(feature = "test-bpf")]

mod helpers;

use helpers::{
    id, program_test,
    stakepool_account::{get_token_balance, transfer},
    LidoAccounts,
};
use lido::{
    instruction,
    token::{Lamports, StLamports},
};
use solana_program::hash::Hash;
use solana_program_test::{tokio, BanksClient};
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::Transaction,
};

use crate::helpers::create_token_account;

async fn setup() -> (BanksClient, Keypair, Hash, LidoAccounts) {
    let (mut banks_client, payer, recent_blockhash) = program_test().start().await;
    let mut lido_accounts = LidoAccounts::new();
    lido_accounts
        .initialize_lido(&mut banks_client, &payer, &recent_blockhash)
        .await
        .unwrap();

    (banks_client, payer, recent_blockhash, lido_accounts)
}
pub const TEST_DEPOSIT_AMOUNT: Lamports = Lamports(1000);

#[tokio::test]
async fn test_successful_deposit() {
    let (mut banks_client, payer, recent_blockhash, lido_accounts) = setup().await;

    let user = Keypair::new();
    let recipient = Keypair::new();

    create_token_account(
        &mut banks_client,
        &payer,
        &recent_blockhash,
        &recipient,
        &lido_accounts.st_sol_mint.pubkey(),
        &user.pubkey(),
    )
    .await
    .unwrap();

    transfer(
        &mut banks_client,
        &payer,
        &recent_blockhash,
        &user.pubkey(),
        TEST_DEPOSIT_AMOUNT,
    )
    .await;

    let mut transaction = Transaction::new_with_payer(
        &[instruction::deposit(
            &id(),
            &instruction::DepositAccountsMeta {
                lido: lido_accounts.lido.pubkey(),
                user: user.pubkey(),
                recipient: recipient.pubkey(),
                st_sol_mint: lido_accounts.st_sol_mint.pubkey(),
                reserve_account: lido_accounts.reserve_authority,
            },
            TEST_DEPOSIT_AMOUNT,
        )
        .unwrap()],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[&payer, &user], recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();

    let balance = StLamports(get_token_balance(&mut banks_client, &recipient.pubkey()).await);

    let reserve_account = banks_client
        .get_account(lido_accounts.reserve_authority)
        .await
        .unwrap()
        .unwrap();

    let rent = banks_client.get_rent().await.unwrap();
    assert_eq!(
        Some(Lamports(reserve_account.lamports)),
        TEST_DEPOSIT_AMOUNT + Lamports(rent.minimum_balance(0))
    );
    // In general, the received stSOL need not be equal to the deposited SOL,
    // but in this particular case, the exchange rate is 1, so this holds.
    assert_eq!(balance.0, TEST_DEPOSIT_AMOUNT.0);
}
