#![cfg(feature = "test-bpf")]

mod helpers;

use helpers::{
    program_test, simple_add_validator_to_pool, stakepool_account::get_account, LidoAccounts,
};
use lido::state::Lido;
use solana_program::{borsh::try_from_slice_unchecked, hash::Hash, native_token::sol_to_lamports};
use solana_program_test::{tokio, BanksClient};
use solana_sdk::signature::{Keypair, Signer};
use spl_stake_pool::stake_program;

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

    let validator_stake =
        simple_add_validator_to_pool(&mut banks_client, &payer, &last_blockhash, &lido_accounts)
            .await;

    let lido_account = get_account(&mut banks_client, &lido_accounts.lido.pubkey()).await;
    let lido = try_from_slice_unchecked::<Lido>(lido_account.data.as_slice()).unwrap();

    let has_stake_account = lido
        .validators
        .entries
        .iter()
        .any(|(v, _)| v == &validator_stake.stake_account);
    // Validator is inside the credit structure
    assert!(has_stake_account);

    let has_token_account = lido
        .validators
        .entries
        .iter()
        .any(|(_, v)| v.fee_address == validator_stake.validator_token_account.pubkey());
    // Validator token account is the same one as provided
    assert!(has_token_account);

    assert_eq!(
        lido.validators.entries.len(),
        1,
    );

    let stake_account = get_account(&mut banks_client, &validator_stake.stake_account).await;
    let stake_account =
        try_from_slice_unchecked::<stake_program::StakeState>(&stake_account.data).unwrap();
    let (meta, _) = match stake_account {
        stake_program::StakeState::Stake(meta, stake) => (meta, stake),
        _ => panic!(),
    };
    let balance = banks_client
        .get_balance(validator_stake.stake_account)
        .await
        .unwrap();
    let rent = banks_client.get_rent().await.unwrap();
    let rent_exempt_validator_stake =
        rent.minimum_balance(std::mem::size_of::<stake_program::StakeState>());
    // Sanity check
    assert_eq!(rent_exempt_validator_stake, meta.rent_exempt_reserve);
    // Stake account balance should have 1 Sol + rent exempt
    assert_eq!(balance, sol_to_lamports(1.) + rent_exempt_validator_stake);
}
