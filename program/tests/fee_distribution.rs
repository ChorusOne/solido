#![cfg(feature = "test-bpf")]

mod helpers;

use borsh::BorshDeserialize;
use helpers::{
    get_account, program_test, simple_add_validator_to_pool,
    stakepool_account::{get_token_balance, transfer, ValidatorStakeAccount},
    LidoAccounts,
};
use solana_program::pubkey::Pubkey;
use solana_program_test::{tokio, ProgramTestContext};
use solana_sdk::signature::Signer;
use spl_stake_pool::state::StakePool;

use lido::token::{Lamports, StLamports, StakePoolTokenLamports};

async fn setup() -> (ProgramTestContext, LidoAccounts, Vec<ValidatorStakeAccount>) {
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

    let mut stake_accounts = Vec::new();
    for _ in 0..NUMBER_VALIDATORS {
        let validator_stake_account = simple_add_validator_to_pool(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &lido_accounts,
        )
        .await;

        stake_accounts.push(validator_stake_account);
    }
    (context, lido_accounts, stake_accounts)
}
const NUMBER_VALIDATORS: u64 = 4;
const TEST_DEPOSIT_AMOUNT: Lamports = Lamports(100_000_000_000);
const EXTRA_STAKE_AMOUNT: Lamports = Lamports(50_000_000_000);

#[tokio::test]
async fn test_successful_fee_distribution() {
    let (mut context, lido_accounts, stake_accounts) = setup().await;

    lido_accounts
        .deposit(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            TEST_DEPOSIT_AMOUNT,
        )
        .await;

    // Delegate the deposit
    let validator_account = stake_accounts.get(0).unwrap();
    let validator_stake = lido_accounts
        .stake_deposit(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            validator_account,
            TEST_DEPOSIT_AMOUNT,
        )
        .await;

    lido_accounts
        .deposit_active_stake_to_pool(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            validator_account,
            &validator_stake,
        )
        .await;

    // Make `EXTRA_STAKE_AMOUNT` appear in every validator account, to simulate
    // validation rewards being paid out.
    for stake_account in &stake_accounts {
        transfer(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &stake_account.stake_pool_stake_account,
            EXTRA_STAKE_AMOUNT,
        )
        .await;
    }

    // Before the update, the fee account that rewards get paid into by the
    // update, should be empty.
    let fee_account_balance_before = get_token_balance(
        &mut context.banks_client,
        &lido_accounts.stake_pool_accounts.pool_fee_account.pubkey(),
    )
    .await;
    assert_eq!(fee_account_balance_before, 0);

    let stake_pool = get_account(
        &mut context.banks_client,
        &lido_accounts.stake_pool_accounts.stake_pool.pubkey(),
    )
    .await;
    let stake_pool = StakePool::try_from_slice(&stake_pool.data.as_slice()).unwrap();

    // The total reward is the the sum of what each stake account received.
    let reward_lamports = (EXTRA_STAKE_AMOUNT * NUMBER_VALIDATORS).unwrap();

    // Of that reward, Lido claims a fraction as fee.
    let fee_stake_pool_tokens_expected =
        StakePoolTokenLamports(stake_pool.calc_fee_amount(reward_lamports.0).unwrap());

    // Now we are going to warp to the next epoch and actually update the pool
    // balance, which should cause rewards to be minted and deposited into the
    // fee account.
    context.warp_to_slot(50_000).unwrap();
    let error = lido_accounts
        .stake_pool_accounts
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

    let fee_in_stake_pool_tokens = StakePoolTokenLamports(
        get_token_balance(
            &mut context.banks_client,
            &lido_accounts.stake_pool_accounts.pool_fee_account.pubkey(),
        )
        .await,
    );

    assert_eq!(fee_in_stake_pool_tokens, fee_stake_pool_tokens_expected);

    let fee_error = lido_accounts
        .distribute_fees(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
        )
        .await;
    assert!(fee_error.is_none());

    let treasury_token_account = get_token_balance(
        &mut context.banks_client,
        &lido_accounts.treasury_account.pubkey(),
    )
    .await;
    let manager_token_account = get_token_balance(
        &mut context.banks_client,
        &lido_accounts.developer_account.pubkey(),
    )
    .await;

    let calculated_fee_distribution = lido::state::distribute_fees(
        &lido_accounts.fee_distribution,
        NUMBER_VALIDATORS,
        fee_stake_pool_tokens_expected,
    )
    .unwrap();

    assert_eq!(
        StLamports(treasury_token_account),
        calculated_fee_distribution.treasury_amount,
    );
    assert_eq!(
        StLamports(manager_token_account),
        calculated_fee_distribution.developer_amount,
    );

    let validator_token_accounts: Vec<Pubkey> = stake_accounts
        .iter()
        .map(|stake_accounts| stake_accounts.validator_token_account.pubkey())
        .collect();

    // Claim validator fees
    for validator_token_account in validator_token_accounts.iter() {
        lido_accounts
            .claim_validator_fees(
                &mut context.banks_client,
                &context.payer,
                &context.last_blockhash,
                validator_token_account,
            )
            .await
            .unwrap();
    }

    for val_acc in &validator_token_accounts {
        assert_eq!(
            calculated_fee_distribution.reward_per_validator,
            StLamports(get_token_balance(&mut context.banks_client, val_acc).await)
        );
    }
    // Should mint rewards only once, balances should be the same
    for validator_token_account in validator_token_accounts.iter() {
        lido_accounts
            .claim_validator_fees(
                &mut context.banks_client,
                &context.payer,
                &context.last_blockhash,
                validator_token_account,
            )
            .await
            .unwrap();
    }
    for val_acc in validator_token_accounts {
        assert_eq!(
            calculated_fee_distribution.reward_per_validator,
            StLamports(get_token_balance(&mut context.banks_client, &val_acc).await)
        );
    }
}
