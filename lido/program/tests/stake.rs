mod helpers;

use helpers::{
    program_test,
    stakepool_account::{
        create_token_account, get_account, get_token_balance, simple_add_validator_to_pool,
        transfer, ValidatorStakeAccount,
    },
    LidoAccounts,
};
use lido::{id, instruction, state};
use solana_program::{borsh::get_packed_len, hash::Hash};
use solana_program_test::{tokio, BanksClient};
use solana_sdk::{
    signature::{Keypair, Signer},
    transaction::Transaction,
};

async fn setup() -> (
    BanksClient,
    Keypair,
    Hash,
    LidoAccounts,
    Vec<ValidatorStakeAccount>,
) {
    let (mut banks_client, payer, recent_blockhash) = program_test().start().await;
    let mut lido_accounts = LidoAccounts::new();
    lido_accounts
        .initialize_lido(&mut banks_client, &payer, &recent_blockhash)
        .await
        .unwrap();

    let validator = simple_add_validator_to_pool(
        &mut banks_client,
        &payer,
        &recent_blockhash,
        &lido_accounts.stake_pool_accounts,
    )
    .await;
    (
        banks_client,
        payer,
        recent_blockhash,
        lido_accounts,
        vec![validator],
    )
}
pub const TEST_DEPOSIT_AMOUNT: u64 = 100_000_000_000;

#[tokio::test]
async fn test_successful_stake() {
    let (mut banks_client, payer, recent_blockhash, lido_accounts, validators) = setup().await;

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
            &lido_accounts.authority,
            &lido_accounts.reserve.pubkey(),
            TEST_DEPOSIT_AMOUNT,
        )
        .unwrap()],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[&payer, &user], recent_blockhash);
    banks_client.process_transaction(transaction).await.unwrap();
}
