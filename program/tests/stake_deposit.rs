#![cfg(feature = "test-bpf")]

mod helpers;

use borsh::BorshDeserialize;
use helpers::{
    get_account, program_test, simple_add_validator_to_pool,
    stakepool_account::{get_token_balance, ValidatorStakeAccount},
    LidoAccounts,
};
use lido::token::{Lamports, StakePoolTokenLamports};
use solana_program::{borsh::try_from_slice_unchecked, hash::Hash};
use solana_program_test::{tokio, BanksClient};
use solana_sdk::signature::{Keypair, Signer};
use spl_stake_pool::{
    minimum_stake_lamports, stake_program,
    state::{StakePool, ValidatorList},
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

    let validator =
        simple_add_validator_to_pool(&mut banks_client, &payer, &recent_blockhash, &lido_accounts)
            .await;

    (
        banks_client,
        payer,
        recent_blockhash,
        lido_accounts,
        vec![validator],
    )
}
pub const TEST_DEPOSIT_AMOUNT: Lamports = Lamports(100_000_000_000);
pub const TEST_STAKE_DEPOSIT_AMOUNT: Lamports = Lamports(10_000_000_000);

#[tokio::test]
async fn test_successful_stake_deposit_stake_pool_deposit() {
    let (mut banks_client, payer, recent_blockhash, lido_accounts, stake_accounts) = setup().await;
    lido_accounts
        .deposit(
            &mut banks_client,
            &payer,
            &recent_blockhash,
            TEST_DEPOSIT_AMOUNT,
        )
        .await;

    // Delegate the deposit
    let validator_account = stake_accounts.get(0).unwrap();
    let stake_account = lido_accounts
        .stake_deposit(
            &mut banks_client,
            &payer,
            &recent_blockhash,
            validator_account,
            TEST_STAKE_DEPOSIT_AMOUNT,
        )
        .await;

    let stake_pool_before = get_account(
        &mut banks_client,
        &lido_accounts.stake_pool_accounts.stake_pool.pubkey(),
    )
    .await;
    let stake_pool_before = StakePool::try_from_slice(&stake_pool_before.data.as_slice()).unwrap();

    let validator_list = get_account(
        &mut banks_client,
        &lido_accounts.stake_pool_accounts.validator_list.pubkey(),
    )
    .await;

    let validator_list =
        try_from_slice_unchecked::<ValidatorList>(validator_list.data.as_slice()).unwrap();
    let validator_stake_item_before = validator_list
        .find(&validator_account.vote.pubkey())
        .unwrap();

    lido_accounts
        .deposit_active_stake_to_pool(
            &mut banks_client,
            &payer,
            &recent_blockhash,
            validator_account,
            &stake_account,
        )
        .await;

    // Stake pool should add its balance to the pool balance
    let stake_pool = get_account(
        &mut banks_client,
        &lido_accounts.stake_pool_accounts.stake_pool.pubkey(),
    )
    .await;
    let stake_pool = StakePool::try_from_slice(&stake_pool.data.as_slice()).unwrap();
    assert_eq!(
        Some(Lamports(stake_pool.total_stake_lamports)),
        Lamports(stake_pool_before.total_stake_lamports) + TEST_STAKE_DEPOSIT_AMOUNT
    );
    // In general we can't add the deposit in SOL and the token supply in stake
    // pool tokens, but in this case, the exchange rate is 1.
    assert_eq!(
        StakePoolTokenLamports(stake_pool.pool_token_supply),
        StakePoolTokenLamports(stake_pool_before.pool_token_supply + TEST_STAKE_DEPOSIT_AMOUNT.0)
    );

    // Check minted tokens
    let lido_token_balance = StakePoolTokenLamports(
        get_token_balance(
            &mut banks_client,
            &lido_accounts.stake_pool_token_holder.pubkey(),
        )
        .await,
    );
    // In general we can't compare stake pool tokens to SOL, but in this case,
    // the exchange rate is 1.
    assert_eq!(lido_token_balance.0, TEST_STAKE_DEPOSIT_AMOUNT.0);

    // Check balances in validator stake account list storage
    let validator_list = get_account(
        &mut banks_client,
        &lido_accounts.stake_pool_accounts.validator_list.pubkey(),
    )
    .await;
    let validator_list =
        try_from_slice_unchecked::<ValidatorList>(validator_list.data.as_slice()).unwrap();
    let validator_stake_item = validator_list
        .find(&validator_account.vote.pubkey())
        .unwrap();
    assert_eq!(
        Some(Lamports(validator_stake_item.stake_lamports)),
        Lamports(validator_stake_item_before.stake_lamports) + TEST_STAKE_DEPOSIT_AMOUNT
    );

    // Check validator stake account actual SOL balance
    let validator_stake_account = get_account(
        &mut banks_client,
        &validator_account.stake_pool_stake_account,
    )
    .await;
    let stake_state =
        bincode::deserialize::<stake_program::StakeState>(&validator_stake_account.data).unwrap();
    let meta = stake_state.meta().unwrap();
    assert_eq!(
        Lamports(validator_stake_account.lamports) - Lamports(minimum_stake_lamports(&meta)),
        Some(Lamports(validator_stake_item.stake_lamports))
    );
}
#[tokio::test]
async fn test_stake_exists_stake_deposit() {} // TODO
