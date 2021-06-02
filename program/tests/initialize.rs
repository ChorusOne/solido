#![cfg(feature = "test-bpf")]

mod helpers;

use helpers::{
    id, program_test, stakepool_account::get_account, LidoAccounts, MAX_MAINTAINERS, MAX_VALIDATORS,
};
use lido::state::{Maintainers, Validators, LIDO_CONSTANT_SIZE};
use solana_program::borsh::get_instance_packed_len;
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
    assert_eq!(
        lido.data.len(),
        LIDO_CONSTANT_SIZE
            + get_instance_packed_len(&Validators::new_fill_default(MAX_VALIDATORS))
                .unwrap()
            + get_instance_packed_len(&Maintainers::new_fill_default(MAX_MAINTAINERS)).unwrap()
    );
    assert_eq!(lido.owner, id());
}

#[tokio::test]
#[should_panic]
async fn test_uninitialize_lido_throws_when_getting_account() {
    let (mut banks_client, _payer, _recent_blockhash) = program_test().start().await;
    let mut _lido_accounts = LidoAccounts::new();

    let _lido = get_account(&mut banks_client, &_lido_accounts.lido.pubkey()).await;
}
