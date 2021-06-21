#![cfg(feature = "test-bpf")]

mod helpers;

use helpers::{get_account, program_test, simple_add_validator_to_pool, LidoAccounts};
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
async fn test_successful_add_validator() {
    let (mut banks_client, payer, last_blockhash, lido_accounts) = setup().await;

    let accounts =
        simple_add_validator_to_pool(&mut banks_client, &payer, &last_blockhash, &lido_accounts)
            .await;

    let lido_account = get_account(&mut banks_client, &lido_accounts.lido.pubkey()).await;
    let lido = try_from_slice_unchecked::<Lido>(lido_account.data.as_slice()).unwrap();

    let has_stake_account = lido.validators.get(&accounts.vote_account.pubkey()).is_ok();

    // Validator is inside the credit structure
    assert!(has_stake_account);

    let has_token_account = lido
        .validators
        .entries
        .iter()
        .any(|pe| pe.entry.fee_address == accounts.fee_account.pubkey());

    // Validator token account is the same one as provided
    assert!(has_token_account);

    assert_eq!(lido.validators.len(), 1);
}
