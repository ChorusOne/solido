mod helpers;

use helpers::{
    program_test,
    stakepool_account::{create_token_account, get_token_balance, transfer},
    LidoAccounts,
};
use lido::{id, instruction};
use solana_program::hash::Hash;
use solana_program_test::{tokio, BanksClient};
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::Transaction,
};

async fn setup() -> (BanksClient, Keypair, Hash, LidoAccounts) {
    let (mut banks_client, payer, recent_blockhash) = program_test().start().await;
    let mut lido_accounts = LidoAccounts::new();
    lido_accounts
        .initialize_lido(&mut banks_client, &payer, &recent_blockhash)
        .await
        .unwrap();

    (banks_client, payer, recent_blockhash, lido_accounts)
}
pub const TEST_DEPOSIT_AMOUNT: u64 = 1000;

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
        &lido_accounts.mint_program.pubkey(),
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
            &lido_accounts.lido.pubkey(),
            &lido_accounts.stake_pool_accounts.stake_pool.pubkey(),
            &lido_accounts.owner.pubkey(),
            &user.pubkey(),
            &recipient.pubkey(),
            &lido_accounts.mint_program.pubkey(),
            &lido_accounts.reserve_authority,
            TEST_DEPOSIT_AMOUNT,
        )
        .unwrap()],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[&payer, &user], recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();

    let balance = get_token_balance(&mut banks_client, &recipient.pubkey()).await;

    let reserve_account = banks_client
        .get_account(lido_accounts.reserve_authority)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(reserve_account.lamports, TEST_DEPOSIT_AMOUNT);
    assert_eq!(balance, TEST_DEPOSIT_AMOUNT);
}
