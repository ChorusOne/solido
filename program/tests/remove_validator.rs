#![cfg(feature = "test-bpf")]

mod helpers;

use helpers::{get_account, program_test, simple_add_validator_to_pool, LidoAccounts};
use lido::state::Lido;
use solana_program::{borsh::try_from_slice_unchecked, hash::Hash, pubkey::Pubkey};
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
async fn test_successful_remove_validator() {
    let (mut banks_client, payer, recent_blockhash, lido_accounts) = setup().await;

    let validator_stake =
        simple_add_validator_to_pool(&mut banks_client, &payer, &recent_blockhash, &lido_accounts)
            .await;

    let lido = get_account(&mut banks_client, &lido_accounts.lido.pubkey()).await;
    let lido = try_from_slice_unchecked::<Lido>(lido.data.as_slice()).unwrap();
    assert_eq!(lido.validators.entries.len(), 1);

    let new_authority = Pubkey::new_unique();
    lido_accounts
        .remove_validator(
            &mut banks_client,
            &payer,
            &recent_blockhash,
            &new_authority,
            &validator_stake.stake_pool_stake_account,
            &validator_stake.transient_stake_account,
        )
        .await;

    let lido = get_account(&mut banks_client, &lido_accounts.lido.pubkey()).await;
    let lido = try_from_slice_unchecked::<Lido>(lido.data.as_slice()).unwrap();
    assert_eq!(lido.validators.entries.len(), 0);
}

// TODO(#179) Add Test for Remove Validator with Unclaimed Rewards
#[tokio::test]
async fn test_remove_validator_with_unclaimed_rewards() {}
