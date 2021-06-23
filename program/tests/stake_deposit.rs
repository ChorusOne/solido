#![cfg(feature = "test-bpf")]

mod helpers;

use helpers::{
    get_account, program_test, simple_add_validator_to_pool, LidoAccounts, ValidatorAccounts,
};
use lido::token::Lamports;
use solana_program_test::{tokio, ProgramTestContext};
use solana_sdk::signature::Signer;

async fn setup() -> (ProgramTestContext, LidoAccounts, ValidatorAccounts) {
    let mut context = program_test().start_with_context().await;
    let mut lido_accounts = LidoAccounts::new();
    lido_accounts.initialize_lido(&mut context).await;
    let validator = simple_add_validator_to_pool(&mut context, &lido_accounts).await;

    (context, lido_accounts, validator)
}
pub const TEST_DEPOSIT_AMOUNT: Lamports = Lamports(100_000_000_000);
pub const TEST_STAKE_DEPOSIT_AMOUNT: Lamports = Lamports(10_000_000_000);

#[tokio::test]
async fn test_successful_stake_deposit() {
    let (mut context, lido_accounts, validator_accounts) = setup().await;

    // Sanity check before we start: the validator should have zero balance in zero stake accounts.
    let solido_before = lido_accounts.get_solido(&mut context).await;
    let validator_before = &solido_before.validators.entries[0].entry;
    assert_eq!(validator_before.stake_accounts_balance, Lamports(0));
    assert_eq!(validator_before.stake_accounts_seed_begin, 0);
    assert_eq!(validator_before.stake_accounts_seed_end, 0);

    // Now we make a deposit, and then delegate part of it.
    lido_accounts
        .deposit(&mut context, TEST_DEPOSIT_AMOUNT)
        .await;

    let stake_account = lido_accounts
        .stake_deposit(
            &mut context,
            &validator_accounts.vote_account.pubkey(),
            TEST_STAKE_DEPOSIT_AMOUNT,
        )
        .await;

    // The amount that we staked, should now be in the stake account.
    assert_eq!(
        Lamports(
            get_account(&mut context.banks_client, &stake_account)
                .await
                .lamports
        ),
        TEST_STAKE_DEPOSIT_AMOUNT,
    );

    // We should also have recorded in the Solido state that this validator now
    // has balance in a stake account.
    let solido_after = lido_accounts.get_solido(&mut context).await;

    let validator_after = &solido_after.validators.entries[0].entry;
    assert_eq!(
        validator_after.stake_accounts_balance,
        TEST_STAKE_DEPOSIT_AMOUNT
    );

    // This was also the first deposit, so that should have created one stake account.
    assert_eq!(validator_after.stake_accounts_seed_begin, 0);
    assert_eq!(validator_after.stake_accounts_seed_end, 1);
}

#[tokio::test]
// TODO(#187) Implement test for stake_exists_stake_deposit
async fn test_stake_exists_stake_deposit() {}
