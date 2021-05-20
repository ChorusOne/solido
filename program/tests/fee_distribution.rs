#![cfg(feature = "test-bpf")]

mod helpers;

use borsh::BorshDeserialize;
use helpers::{
    program_test, simple_add_validator_to_pool,
    stakepool_account::{get_account, get_token_balance, transfer, ValidatorStakeAccount},
    LidoAccounts,
};
use solana_program::pubkey::Pubkey;
use solana_program_test::{tokio, ProgramTestContext};
use solana_sdk::signature::Signer;

use spl_stake_pool::state::StakePool;

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
    for _ in 0..STAKE_ACCOUNTS {
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
const STAKE_ACCOUNTS: u32 = 4;
const TEST_DEPOSIT_AMOUNT: u64 = 100_000_000_000;
const EXTRA_STAKE_AMOUNT: u64 = 50_000_000_000;

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
        .delegate_deposit(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            validator_account,
            TEST_DEPOSIT_AMOUNT,
        )
        .await;

    lido_accounts
        .delegate_stakepool_deposit(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            validator_account,
            &validator_stake,
        )
        .await;

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

    context.warp_to_slot(50_000).unwrap();

    // Update list and pool
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
    let fee_error = lido_accounts
        .distribute_fees(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
        )
        .await;
    assert!(fee_error.is_none());

    let insurance_token_amount = get_token_balance(
        &mut context.banks_client,
        &lido_accounts.insurance_account.pubkey(),
    )
    .await;
    let treasury_token_account = get_token_balance(
        &mut context.banks_client,
        &lido_accounts.treasury_account.pubkey(),
    )
    .await;
    let manager_token_account = get_token_balance(
        &mut context.banks_client,
        &lido_accounts.manager_fee_account.pubkey(),
    )
    .await;
    let total_fees = ((STAKE_ACCOUNTS as u128
        * EXTRA_STAKE_AMOUNT as u128
        * lido_accounts.stake_pool_accounts.fee.numerator as u128)
        / lido_accounts.stake_pool_accounts.fee.denominator as u128) as u64;

    let calculated_fee_structure = lido_accounts
        .fee_structure
        .calculate_token_amounts(total_fees, STAKE_ACCOUNTS)
        .unwrap();

    assert_eq!(
        calculated_fee_structure.insurance_amount,
        insurance_token_amount
    );
    assert_eq!(
        calculated_fee_structure.treasury_amount,
        treasury_token_account
    );
    assert_eq!(
        calculated_fee_structure.manager_amount,
        manager_token_account
    );

    let validator_token_accounts: Vec<Pubkey> = stake_accounts
        .iter()
        .map(|stake_accounts| stake_accounts.validator_token_account.pubkey())
        .collect();
    // Claim validator fees
    lido_accounts
        .claim_validator_fees(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            0,
            &validator_token_accounts,
        )
        .await
        .unwrap();

    for val_acc in &validator_token_accounts {
        assert_eq!(
            calculated_fee_structure.each_validator_amount,
            get_token_balance(&mut context.banks_client, val_acc).await
        );
    }
    // Should mint rewards only once, balances should be the same
    lido_accounts
        .claim_validator_fees(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            0,
            &vec![validator_token_accounts[0]],
        )
        .await
        .unwrap();
    for val_acc in validator_token_accounts {
        assert_eq!(
            calculated_fee_structure.each_validator_amount,
            get_token_balance(&mut context.banks_client, &val_acc).await
        );
    }
}
