#![cfg(feature = "test-bpf")]

mod helpers;

use bincode::deserialize;
use helpers::{
    program_test, simple_add_validator_to_pool,
    stakepool_account::{get_account, ValidatorStakeAccount},
    LidoAccounts,
};
use lido::token::Lamports;
use solana_program::epoch_schedule::Epoch;
use solana_program_test::{tokio, ProgramTestContext};
use solana_sdk::signature::Signer;
use spl_stake_pool::stake_program;

const TEST_DEPOSIT_AMOUNT: Lamports = Lamports(100_000_000_000);
const TEST_DECREASE_AMOUNT: Lamports = Lamports(100_000_000_000 / 2);
async fn setup() -> (ProgramTestContext, LidoAccounts, ValidatorStakeAccount) {
    let mut context = program_test().start_with_context().await;
    let mut lido_accounts = LidoAccounts::new();
    lido_accounts
        .initialize_lido(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
        )
        .await
        .unwrap();
    let validator_account = simple_add_validator_to_pool(
        &mut context.banks_client,
        &context.payer,
        &context.last_blockhash,
        &lido_accounts,
    )
    .await;

    lido_accounts
        .deposit(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            TEST_DEPOSIT_AMOUNT,
        )
        .await;

    // Delegate the deposit
    let stake_account = lido_accounts
        .stake_deposit(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &validator_account,
            TEST_DEPOSIT_AMOUNT,
        )
        .await;

    lido_accounts
        .deposit_active_stake_to_pool(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &validator_account,
            &stake_account,
        )
        .await;

    (context, lido_accounts, validator_account)
}

/// Copied from Stake pool program
#[tokio::test]
async fn test_successful_decrease_validator_stake() {
    let (mut context, lido_accounts, stake_accounts) = setup().await;

    // Save validator stake
    let pre_validator_stake_account =
        get_account(&mut context.banks_client, &stake_accounts.stake_account).await;
    // Check no transient stake
    let transient_account = context
        .banks_client
        .get_account(stake_accounts.transient_stake_account)
        .await
        .unwrap();
    assert!(transient_account.is_none());

    let result = lido_accounts
        .decrease_validator_stake(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &stake_accounts.transient_stake_account,
            &stake_accounts.stake_account,
            TEST_DECREASE_AMOUNT,
        )
        .await;
    assert!(result.is_ok());

    let validator_stake_account =
        get_account(&mut context.banks_client, &stake_accounts.stake_account).await;
    let validator_stake_state =
        deserialize::<stake_program::StakeState>(&validator_stake_account.data).unwrap();
    assert_eq!(
        Lamports(pre_validator_stake_account.lamports) - TEST_DECREASE_AMOUNT,
        Some(Lamports(validator_stake_account.lamports))
    );
    assert_eq!(
        validator_stake_state
            .delegation()
            .unwrap()
            .deactivation_epoch,
        Epoch::MAX
    );

    // Check transient stake account state and balance
    let transient_stake_account = get_account(
        &mut context.banks_client,
        &stake_accounts.transient_stake_account,
    )
    .await;
    let transient_stake_state =
        deserialize::<stake_program::StakeState>(&transient_stake_account.data).unwrap();
    assert_eq!(
        Lamports(transient_stake_account.lamports),
        TEST_DECREASE_AMOUNT
    );
    assert_ne!(
        transient_stake_state
            .delegation()
            .unwrap()
            .deactivation_epoch,
        Epoch::MAX
    );
}

/// Copied from Stake pool program
#[tokio::test]
async fn test_successful_increase_validator_stake() {
    let (mut context, lido_accounts, stake_accounts) = setup().await;
    // Check no transient stake
    let transient_account = context
        .banks_client
        .get_account(stake_accounts.transient_stake_account)
        .await
        .unwrap();
    assert!(transient_account.is_none());

    let result = lido_accounts
        .decrease_validator_stake(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &stake_accounts.transient_stake_account,
            &stake_accounts.stake_account,
            TEST_DECREASE_AMOUNT,
        )
        .await;
    assert!(result.is_ok());

    // Warp to next epoch, so funds are in the reserve
    context.warp_to_slot(50_000).unwrap();
    let error = lido_accounts
        .stake_pool_accounts
        .update_all(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &[stake_accounts.vote.pubkey()],
            false,
        )
        .await;
    assert!(error.is_none());

    // Save reserve stake
    let pre_reserve_stake_account = get_account(
        &mut context.banks_client,
        &lido_accounts.stake_pool_accounts.reserve_stake.pubkey(),
    )
    .await;

    let rent = context.banks_client.get_rent().await.unwrap();
    let lamports = Lamports(rent.minimum_balance(std::mem::size_of::<stake_program::StakeState>()));
    let reserve_lamports = (TEST_DECREASE_AMOUNT - lamports).unwrap();
    let result = lido_accounts
        .increase_validator_stake(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &stake_accounts.transient_stake_account,
            &stake_accounts.vote.pubkey(),
            reserve_lamports,
        )
        .await;
    assert!(result.is_ok());

    // Check reserve stake account balance
    let reserve_stake_account = get_account(
        &mut context.banks_client,
        &lido_accounts.stake_pool_accounts.reserve_stake.pubkey(),
    )
    .await;
    let reserve_stake_state =
        deserialize::<stake_program::StakeState>(&reserve_stake_account.data).unwrap();
    assert_eq!(
        Lamports(pre_reserve_stake_account.lamports) - reserve_lamports,
        Some(Lamports(reserve_stake_account.lamports))
    );
    assert!(reserve_stake_state.delegation().is_none());

    // Check transient stake account state and balance
    let transient_stake_account = get_account(
        &mut context.banks_client,
        &stake_accounts.transient_stake_account,
    )
    .await;
    let transient_stake_state =
        deserialize::<stake_program::StakeState>(&transient_stake_account.data).unwrap();
    assert_eq!(Lamports(transient_stake_account.lamports), reserve_lamports);
    assert_ne!(
        transient_stake_state.delegation().unwrap().activation_epoch,
        Epoch::MAX
    );
}
