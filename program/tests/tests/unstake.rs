// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use testlib::assert_solido_error;
use testlib::solido_context::{self, Context};

use lido::processor::StakeType;
use lido::state::{ListEntry, StakeDeposit};
use lido::MINIMUM_STAKE_ACCOUNT_BALANCE;
use lido::{error::LidoError, token::Lamports};
use solana_program::stake::state::StakeState;
use solana_program_test::tokio;
use solana_sdk::instruction::InstructionError;
use solana_sdk::signer::Signer;
use solana_sdk::transaction::TransactionError;
use solana_sdk::transport::TransportError;

const STAKE_AMOUNT: Lamports = Lamports(10_000_000_000);

// Shorthand to check for this error in tests.
fn is_insufficient_funds_error<T>(result: Result<T, TransportError>) -> bool {
    match result {
        Err(TransportError::TransactionError(TransactionError::InstructionError(
            _,
            InstructionError::InsufficientFunds,
        ))) => true,
        _ => false,
    }
}

/// Set up a Solido instance with one validator that has active stake accounts.
///
/// There will be one stake account for every element of `stake_amounts`.
async fn new_unstake_context(stake_amounts: &[Lamports]) -> Context {
    let mut context = Context::new_with_maintainer_and_validator().await;
    let vote_account = context.validator.as_ref().unwrap().vote_account;

    // Set up stake accounts with the given amounts.
    for stake_amount in stake_amounts {
        context.deposit(*stake_amount).await;
        context
            .stake_deposit(vote_account, StakeDeposit::Append, *stake_amount)
            .await;
    }

    // Wait for the stake to activate.
    context.advance_to_normal_epoch(0);
    context.update_exchange_rate().await;

    context
}

#[tokio::test]
async fn test_successful_unstake() {
    let mut context = new_unstake_context(&[STAKE_AMOUNT]).await;
    let unstake_lamports = Lamports(1_000_000_000);

    let solido = context.get_solido().await;
    let validator = &solido.validators.entries[0];

    let stake_account_before = context.get_stake_account_from_seed(&validator, 0).await;
    context.unstake(*validator.pubkey(), unstake_lamports).await;
    let stake_account_after = context.get_stake_account_from_seed(&validator, 0).await;
    assert_eq!(
        (stake_account_before.balance.total() - stake_account_after.balance.total()).unwrap(),
        unstake_lamports
    );
    let unstake_account = context.get_unstake_account_from_seed(&validator, 0).await;

    let rent = context.get_rent().await;
    let stake_rent = rent.minimum_balance(std::mem::size_of::<StakeState>());
    // The rent will not become deactivated.
    assert_eq!(
        unstake_account.balance.deactivating,
        (unstake_lamports - Lamports(stake_rent)).unwrap()
    );
}

#[tokio::test]
async fn test_unstake_requires_unstaking_at_least_the_rent_exempt_amount() {
    let mut context = new_unstake_context(&[STAKE_AMOUNT]).await;
    let vote_account = context.validator.as_ref().unwrap().vote_account;

    let rent = context.get_rent().await;
    let stake_rent = rent.minimum_balance(std::mem::size_of::<StakeState>());

    // Unstaking less than the rent should fail, because the target account would
    // not be rent-exempt.
    let result = context
        .try_unstake(vote_account, Lamports(stake_rent - 1))
        .await;
    assert!(is_insufficient_funds_error(result));

    // Unstaking just the rent-exempt amount is also disallowed by the stake
    // program, but unstaking one lamport more is allowed.
    context
        .unstake(vote_account, Lamports(stake_rent + 1))
        .await;
}

#[tokio::test]
async fn test_unstake_leaves_minimum_stake_account_balance_for_active_validator() {
    let mut context = new_unstake_context(&[STAKE_AMOUNT, STAKE_AMOUNT]).await;
    let vote_account = context.validator.as_ref().unwrap().vote_account;

    // Both stake accounts should have STAKE_AMOUNT in them, so despite having
    // more than `max_to_unstake` in stake accounts in total, we can only unstake
    // this much at a time.
    let max_to_unstake = (STAKE_AMOUNT - MINIMUM_STAKE_ACCOUNT_BALANCE).unwrap();

    // We can't let the stake account balance drop below the minimum.
    let result = context
        .try_unstake(vote_account, Lamports(max_to_unstake.0 + 1))
        .await;
    assert_solido_error!(result, LidoError::InvalidAmount);

    // Just the maximum should work.
    context.unstake(vote_account, max_to_unstake).await;
}

#[tokio::test]
async fn test_unstake_more_than_stake_account_balance_fails() {
    let mut context = new_unstake_context(&[STAKE_AMOUNT]).await;
    let vote_account = context.validator.as_ref().unwrap().vote_account;

    // We can't unstake more than what's in the stake account. This is already
    // disallowed because we'd leave the account with less than the minimum stake
    // account balance, but we still test this to check overflow handling.
    let result = context
        .try_unstake(vote_account, Lamports(STAKE_AMOUNT.0 + 1))
        .await;
    assert!(is_insufficient_funds_error(result));
}

#[tokio::test]
async fn test_unstake_from_inactive_validator() {
    let mut context = new_unstake_context(&[STAKE_AMOUNT, STAKE_AMOUNT]).await;
    let vote_account = context.validator.as_ref().unwrap().vote_account;

    context.deactivate_validator(vote_account).await;

    // We should not be allowed to unstake less than the full stake account balance.
    // If we only try to unstake one lamport less, this is disallowed because it
    // would leave the source account not rent-exempt.
    let result = context
        .try_unstake(vote_account, Lamports(STAKE_AMOUNT.0 - 1))
        .await;
    assert!(is_insufficient_funds_error(result));

    // If we leave more behind, this is disallowed because we check for it.
    let result = context
        .try_unstake(vote_account, Lamports(STAKE_AMOUNT.0 / 2))
        .await;
    assert_solido_error!(result, LidoError::InvalidAmount);

    // We can't unstake more either. This is a different case than
    // `test_unstake_more_than_stake_account_balance_fails()`, because for
    // inactive validators we do not need to leave the minimum stake account
    // balance behind.
    let result = context
        .try_unstake(vote_account, Lamports(STAKE_AMOUNT.0 + 1))
        .await;
    assert!(is_insufficient_funds_error(result));

    // But unstaking exactly the stake account balance should work.
    let solido_before = context.get_solido().await;
    context.unstake(vote_account, STAKE_AMOUNT).await;
    let solido_after = context.get_solido().await;

    assert_eq!(
        solido_before.validators.entries[0].stake_seeds.begin + 1,
        solido_after.validators.entries[0].stake_seeds.begin,
        "Unstaking the full stake account amount should have bumped the steed.",
    );

    // We should be able to do it a second time and unstake the second stake account.
    context.unstake(vote_account, STAKE_AMOUNT).await;

    let validator = &context.get_solido().await.validators.entries[0];
    assert_eq!(
        validator.stake_seeds.begin, validator.stake_seeds.end,
        "No stake accounts should be left after unstaking both."
    );
    assert_eq!(
        validator.stake_accounts_balance, validator.unstake_accounts_balance,
        "The full balance should be in unstake accounts after unstaking both."
    );
    let (stake_account, _) = validator.find_stake_account_address(
        &solido_context::id(),
        &context.solido.pubkey(),
        validator.stake_seeds.begin,
        StakeType::Stake,
    );
    let account = context.try_get_account(stake_account).await;
    assert!(
        account.is_none(),
        "Former stake account should no longer exist."
    );
}

#[tokio::test]
async fn test_unstake_with_funded_destination_stake() {
    let mut context = new_unstake_context(&[STAKE_AMOUNT]).await;
    let validator = &context.get_solido().await.validators.entries[0];
    let (unstake_address, _) = validator.find_stake_account_address(
        &solido_context::id(),
        &context.solido.pubkey(),
        0,
        StakeType::Unstake,
    );
    context.fund(unstake_address, Lamports(500_000_000)).await;
    let unstake_lamports = Lamports(1_000_000_000);

    context.unstake(*validator.pubkey(), unstake_lamports).await;
    let unstake_account = context.get_unstake_account_from_seed(&validator, 0).await;
    // Since we already had something in the account that paid for the rent, we
    // can unstake all the requested amount.
    assert_eq!(unstake_account.balance.deactivating, unstake_lamports);
}

#[tokio::test]
async fn test_unstake_allows_at_most_three_unstake_accounts() {
    let mut context = new_unstake_context(&[STAKE_AMOUNT]).await;
    let vote_account = context.validator.as_ref().unwrap().vote_account;
    let unstake_amount = Lamports(1_000_000_000);

    // The first three unstakes should be allowed.
    context.unstake(vote_account, unstake_amount).await;
    context.unstake(vote_account, unstake_amount).await;
    context.unstake(vote_account, unstake_amount).await;

    // But the fourth one is not; we can have at most three unstake accounts.
    let result = context.try_unstake(vote_account, unstake_amount).await;
    assert_solido_error!(result, LidoError::MaxUnstakeAccountsReached);

    // Wait for the unstake accounts to deactivate.
    context.advance_to_normal_epoch(1);

    let solido_before = context.get_solido().await;
    let validator_before = &solido_before.validators.entries[0];
    assert_eq!(validator_before.unstake_seeds.begin, 0);
    assert_eq!(validator_before.unstake_seeds.end, 3);

    // Withdraw the now-inactive stake accounts to the reserve to free up
    // unstake accounts again.
    context.update_stake_account_balance(vote_account).await;

    let solido_after = context.get_solido().await;
    let validator_after = &solido_after.validators.entries[0];
    assert_eq!(validator_after.unstake_seeds.begin, 3);
    assert_eq!(validator_after.unstake_seeds.end, 3);

    // Now we should be allowed to unstake again.
    context.unstake(vote_account, unstake_amount).await;
}

#[tokio::test]
async fn test_unstake_activating() {
    let mut context = Context::new_with_maintainer_and_validator().await;
    let unstake_lamports = Lamports(1_000_000_000);

    let solido = context.get_solido().await;
    let validator = &solido.validators.entries[0];

    context.deposit(Lamports(10_000_000_000)).await;
    context
        .stake_deposit(
            *validator.pubkey(),
            StakeDeposit::Append,
            Lamports(10_000_000_000),
        )
        .await;

    let rent = context.get_rent().await;
    let stake_rent = rent.minimum_balance(std::mem::size_of::<StakeState>());

    let stake_account_before = context.get_stake_account_from_seed(&validator, 0).await;
    assert_eq!(stake_account_before.balance.active, Lamports(0));
    assert_eq!(
        stake_account_before.balance.activating,
        (Lamports(10_000_000_000) - Lamports(stake_rent)).unwrap()
    );

    context.unstake(*validator.pubkey(), unstake_lamports).await;
    let stake_account_after = context.get_stake_account_from_seed(&validator, 0).await;
    assert_eq!(
        (stake_account_before.balance.total() - stake_account_after.balance.total()).unwrap(),
        unstake_lamports
    );
    let unstake_account = context.get_unstake_account_from_seed(&validator, 0).await;

    // Unstaking activating Sol will become inactive right away.
    assert_eq!(unstake_account.balance.inactive, unstake_lamports);
}
