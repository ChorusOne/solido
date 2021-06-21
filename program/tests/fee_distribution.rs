#![cfg(feature = "test-bpf")]

mod helpers;

use helpers::{
    program_test, simple_add_validator_to_pool, transfer, LidoAccounts, ValidatorAccounts,
};
use solana_program_test::{tokio, ProgramTestContext};
use solana_sdk::signature::Signer;

use lido::token::Lamports;

async fn setup() -> (ProgramTestContext, LidoAccounts, Vec<ValidatorAccounts>) {
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

    let mut validator_accounts = Vec::new();
    for _ in 0..NUMBER_VALIDATORS {
        let accounts = simple_add_validator_to_pool(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &lido_accounts,
        )
        .await;

        validator_accounts.push(accounts);
    }
    (context, lido_accounts, validator_accounts)
}
const NUMBER_VALIDATORS: u64 = 4;
const TEST_DEPOSIT_AMOUNT: Lamports = Lamports(100_000_000_000);
const EXTRA_STAKE_AMOUNT: Lamports = Lamports(50_000_000_000);

#[tokio::test]
async fn test_successful_fee_distribution() {
    let (mut context, lido_accounts, validators) = setup().await;

    lido_accounts
        .deposit(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            TEST_DEPOSIT_AMOUNT,
        )
        .await;

    // Delegate the deposit.
    let validator_accounts = &validators[0];
    let stake_account = lido_accounts
        .stake_deposit(
            &mut context.banks_client,
            &context.payer,
            &context.last_blockhash,
            &validator_accounts.vote_account.pubkey(),
            TEST_DEPOSIT_AMOUNT,
        )
        .await;

    // Make `EXTRA_STAKE_AMOUNT` appear in the stake account, to simulate
    // validation rewards being paid out.
    // TODO(#207): this may not be the right way to simulate rewards.
    transfer(
        &mut context.banks_client,
        &context.payer,
        &context.last_blockhash,
        &stake_account,
        EXTRA_STAKE_AMOUNT,
    )
    .await;

    // TODO(#178): Restore the remainder of this test, once we implement validator
    // balance updates, and therefore fee distribution.
}
