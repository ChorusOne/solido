#![cfg(feature = "test-bpf")]

mod helpers;

use helpers::{program_test, stakepool_account::get_account, LidoAccounts};
use lido::{id, state};
use solana_program::borsh::get_packed_len;
use solana_program_test::tokio;
use solana_sdk::signature::Signer;

#[tokio::test]
async fn test_success_initialize() {
    let (mut banks_client, payer, recent_blockhash) = program_test().start().await;
    let mut lido_accounts = LidoAccounts::new();
    lido_accounts
        .initialize_lido(&mut banks_client, &payer, &recent_blockhash)
        .await
        .unwrap();

    let lido = get_account(&mut banks_client, &lido_accounts.lido.pubkey()).await;
    assert_eq!(lido.data.len(), get_packed_len::<state::Lido>());
    assert_eq!(lido.owner, id());
}

#[tokio::test]
#[should_panic]
async fn test_uninitialize() {
    let (mut banks_client, payer, recent_blockhash) = program_test().start().await;
    let mut lido_accounts = LidoAccounts::new();

    let lido = get_account(&mut banks_client, &lido_accounts.lido.pubkey()).await;
}