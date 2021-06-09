#![cfg(feature = "test-bpf")]

mod helpers;

use helpers::{
    get_account, program_test, simple_add_maintainer, simple_remove_maintainer, LidoAccounts,
};
use lido::state::Lido;
use solana_program::{borsh::try_from_slice_unchecked, hash::Hash};
use solana_program_test::{tokio, BanksClient};
use solana_sdk::signature::{Keypair, Signer};

async fn setup() -> (BanksClient, Keypair, Hash, LidoAccounts) {
    let (mut banks_client, payer, last_blockhash) = program_test().start().await;
    let mut lido_accounts = LidoAccounts::new();
    lido_accounts
        .initialize_lido(&mut banks_client, &payer, &last_blockhash)
        .await
        .unwrap();
    (banks_client, payer, last_blockhash, lido_accounts)
}

#[tokio::test]
async fn test_successful_add_remove_maintainer() {
    let (mut banks_client, payer, last_blockhash, lido_accounts) = setup().await;

    let maintainer = Keypair::new();
    simple_add_maintainer(
        &mut banks_client,
        &payer,
        &last_blockhash,
        &maintainer.pubkey(),
        &lido_accounts,
    )
    .await
    .unwrap();

    let lido_account = get_account(&mut banks_client, &lido_accounts.lido.pubkey()).await;
    let lido = try_from_slice_unchecked::<Lido>(lido_account.data.as_slice()).unwrap();

    let has_maintainer = lido
        .maintainers
        .entries
        .iter()
        .any(|pe| pe.pubkey == maintainer.pubkey());
    assert!(has_maintainer);
    simple_remove_maintainer(
        &mut banks_client,
        &payer,
        &last_blockhash,
        &lido_accounts,
        &maintainer.pubkey(),
    )
    .await
    .unwrap();

    let lido_account = get_account(&mut banks_client, &lido_accounts.lido.pubkey()).await;
    let lido = try_from_slice_unchecked::<Lido>(lido_account.data.as_slice()).unwrap();

    let has_maintainer = lido
        .maintainers
        .entries
        .iter()
        .any(|pe| pe.pubkey == maintainer.pubkey());
    assert!(!has_maintainer);
}
