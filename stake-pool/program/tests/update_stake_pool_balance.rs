#![cfg(feature = "test-bpf")]

mod helpers;

use {
    borsh::BorshDeserialize,
    helpers::*,
    solana_program::{instruction::InstructionError, pubkey::Pubkey},
    solana_program_test::*,
    solana_sdk::{
        signature::{Keypair, Signer},
        transaction::TransactionError,
    },
    spl_stake_pool::{error::StakePoolError, state::StakePool},
};

async fn setup() -> (
    ProgramTestContext,
    StakePoolAccounts,
    Vec<ValidatorStakeAccount>,
) {
    let mut context = program_test().start_with_context().await;
    let stake_pool_accounts = StakePoolAccounts::new();
    stake_pool_accounts
        .initialize_stake_pool(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            1,
        )
        .await
        .unwrap();

    // Add several accounts
    let mut stake_accounts: Vec<ValidatorStakeAccount> = vec![];
    const STAKE_ACCOUNTS: u64 = 3;
    for _ in 0..STAKE_ACCOUNTS {
        let validator_stake_account = simple_add_validator_to_pool(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &stake_pool_accounts,
        )
        .await;

        let _deposit_info = simple_deposit(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &stake_pool_accounts,
            &validator_stake_account,
            TEST_STAKE_AMOUNT,
        )
        .await
        .unwrap();

        stake_accounts.push(validator_stake_account);
    }

    (context, stake_pool_accounts, stake_accounts)
}

#[tokio::test]
async fn success() {
    let (mut context, stake_pool_accounts, stake_accounts) = setup().await;

    let pre_balance = get_validator_list_sum(
        &mut context.banks_client,
        &stake_pool_accounts.reserve_stake.pubkey(),
        &stake_pool_accounts.validator_list.pubkey(),
    )
    .await;
    let stake_pool = get_account(
        &mut context.banks_client,
        &stake_pool_accounts.stake_pool.pubkey(),
    )
    .await;
    let stake_pool = StakePool::try_from_slice(&stake_pool.data.as_slice()).unwrap();
    assert_eq!(pre_balance, stake_pool.total_stake_lamports);

    let pre_token_supply = get_token_supply(
        &mut context.banks_client,
        &stake_pool_accounts.pool_mint.pubkey(),
    )
    .await;

    let error = stake_pool_accounts
        .update_stake_pool_balance(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
        )
        .await;
    assert!(error.is_none());

    // Add extra funds, simulating rewards
    const EXTRA_STAKE_AMOUNT: u64 = 1_000_000;
    for stake_account in &stake_accounts {
        transfer(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &stake_account.stake_account,
            EXTRA_STAKE_AMOUNT,
        )
        .await;
    }

    // Update epoch
    context.warp_to_slot(50_000).unwrap();

    // Update list and pool
    let error = stake_pool_accounts
        .update_all(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            stake_accounts
                .iter()
                .map(|v| v.vote.pubkey())
                .collect::<Vec<Pubkey>>()
                .as_slice(),
            false,
        )
        .await;
    assert!(error.is_none());

    // Check fee
    let post_balance = get_validator_list_sum(
        &mut context.banks_client,
        &stake_pool_accounts.reserve_stake.pubkey(),
        &stake_pool_accounts.validator_list.pubkey(),
    )
    .await;
    let stake_pool = get_account(
        &mut context.banks_client,
        &stake_pool_accounts.stake_pool.pubkey(),
    )
    .await;
    let stake_pool = StakePool::try_from_slice(&stake_pool.data.as_slice()).unwrap();
    assert_eq!(post_balance, stake_pool.total_stake_lamports);

    let actual_fee = get_token_balance(
        &mut context.banks_client,
        &stake_pool_accounts.pool_fee_account.pubkey(),
    )
    .await;
    let pool_token_supply = get_token_supply(
        &mut context.banks_client,
        &stake_pool_accounts.pool_mint.pubkey(),
    )
    .await;

    let stake_pool_info = get_account(
        &mut context.banks_client,
        &stake_pool_accounts.stake_pool.pubkey(),
    )
    .await;
    let stake_pool = StakePool::try_from_slice(&stake_pool_info.data).unwrap();
    let expected_fee = (post_balance - pre_balance) * pre_token_supply / pre_balance
        * stake_pool.fee.numerator
        / stake_pool.fee.denominator;
    assert_eq!(actual_fee, expected_fee);
    assert_eq!(pool_token_supply, stake_pool.pool_token_supply);
}

#[tokio::test]
async fn fail_with_wrong_validator_list() {
    let (mut banks_client, payer, recent_blockhash) = program_test().start().await;
    let mut stake_pool_accounts = StakePoolAccounts::new();
    stake_pool_accounts
        .initialize_stake_pool(&mut banks_client, &payer, &recent_blockhash, 1)
        .await
        .unwrap();

    let wrong_validator_list = Keypair::new();
    stake_pool_accounts.validator_list = wrong_validator_list;
    let error = stake_pool_accounts
        .update_stake_pool_balance(&mut banks_client, &payer, &recent_blockhash)
        .await
        .unwrap()
        .unwrap();

    match error {
        TransactionError::InstructionError(
            _,
            InstructionError::Custom(error_index),
        ) => {
            let program_error = StakePoolError::InvalidValidatorStakeList as u32;
            assert_eq!(error_index, program_error);
        }
        _ => panic!("Wrong error occurs while try to update pool balance with wrong validator stake list account"),
    }
}

#[tokio::test]
async fn fail_with_wrong_pool_fee_account() {
    let (mut banks_client, payer, recent_blockhash) = program_test().start().await;
    let mut stake_pool_accounts = StakePoolAccounts::new();
    stake_pool_accounts
        .initialize_stake_pool(&mut banks_client, &payer, &recent_blockhash, 1)
        .await
        .unwrap();

    let wrong_fee_account = Keypair::new();
    stake_pool_accounts.pool_fee_account = wrong_fee_account;
    let error = stake_pool_accounts
        .update_stake_pool_balance(&mut banks_client, &payer, &recent_blockhash)
        .await
        .unwrap()
        .unwrap();

    match error {
        TransactionError::InstructionError(
            _,
            InstructionError::Custom(error_index),
        ) => {
            let program_error = StakePoolError::InvalidFeeAccount as u32;
            assert_eq!(error_index, program_error);
        }
        _ => panic!("Wrong error occurs while try to update pool balance with wrong validator stake list account"),
    }
}

#[tokio::test]
async fn test_update_stake_pool_balance_with_uninitialized_validator_list() {} // TODO

#[tokio::test]
async fn test_update_stake_pool_balance_with_out_of_dated_validators_balances() {} // TODO
