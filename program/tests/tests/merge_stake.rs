// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use testlib::assert_solido_error;
use testlib::solido_context::{self, get_account_info, Context};

use lido::processor::StakeType;
use lido::state::{Lido, ListEntry, StakeDeposit};
use lido::{error::LidoError, token::Lamports};
use solana_program_test::tokio;
use solana_sdk::signer::Signer;

#[tokio::test]
async fn test_successful_merge_activating_stake() {
    let (mut context, stake_account_pubkeys) = Context::new_with_two_stake_accounts().await;

    let rent = context.get_rent().await;
    let solido_before = context.get_solido().await;
    let validator = &context.get_solido().await.validators.entries[0];
    let mut reserve_before = context.get_account(context.reserve_address).await;

    context.merge_stake(&validator, 0, 1).await;

    let account = context.try_get_account(stake_account_pubkeys[0]).await;
    assert!(account.is_none());

    let stake = context.get_stake_state(stake_account_pubkeys[1]).await;
    let rent_exempt_reserve = context
        .get_stake_rent_exempt_reserve(stake_account_pubkeys[1])
        .await;
    let sum = 20_000_000_000 - rent_exempt_reserve.0;
    assert_eq!(stake.delegation.stake, sum, "Unexpected delegated stake.");

    let solido_after = context.get_solido().await;
    let mut reserve_after = context.get_account(context.reserve_address).await;
    assert_eq!(
        solido_after.validators.entries[0].stake_accounts_balance,
        Lamports(20_000_000_000)
    );

    let validator_before = &solido_before.validators.entries[0];
    let validator_after = &solido_after.validators.entries[0];
    assert_eq!(
        validator_after.stake_seeds.begin,
        validator_before.stake_seeds.begin + 1,
    );

    let sol_before = Lido::get_sol_balance(
        solido_before.validators.entries.iter(),
        &rent,
        &get_account_info(&context.reserve_address, &mut reserve_before),
    );
    let sol_after = Lido::get_sol_balance(
        solido_after.validators.entries.iter(),
        &rent,
        &get_account_info(&context.reserve_address, &mut reserve_after),
    );
    assert_eq!(
        sol_before, sol_after,
        "Merging should not change the total amount of SOL."
    );
}

// Test merging active to activating: should fail.
// Test merging two activated stake accounts: should succeed.
#[tokio::test]
async fn test_merge_stake_combinations() {
    let stake_deposit_amount = Lamports(2_000_000_000); // 2 Sol
    let mut context = Context::new_with_maintainer_and_validator().await;

    context.advance_to_normal_epoch(0);

    let validator = &context.get_solido().await.validators.entries[0];
    context.deposit(Lamports(100_000_000_000)).await;
    context
        .stake_deposit(
            *validator.pubkey(),
            StakeDeposit::Append,
            stake_deposit_amount,
        )
        .await;

    context.advance_to_normal_epoch(1);

    // Create an activating stake account.
    context
        .stake_deposit(
            *validator.pubkey(),
            StakeDeposit::Append,
            stake_deposit_amount,
        )
        .await;

    let active_stake_account = context.get_stake_account_from_seed(&validator, 0).await;

    let activating_stake_account = context.get_stake_account_from_seed(&validator, 1).await;

    assert!(active_stake_account.is_active());
    assert!(activating_stake_account.is_activating());
    let result = context.try_merge_stake(&validator, 0, 1).await;
    // Merging active to activating should fail.
    assert_solido_error!(result, LidoError::WrongStakeState);

    context.advance_to_normal_epoch(2);

    let now_active_stake_account = context.get_stake_account_from_seed(&validator, 1).await;
    assert!(active_stake_account.is_active());
    assert!(now_active_stake_account.is_active());

    let rent = context.get_rent().await;
    let solido_before = context.get_solido().await;
    let mut reserve_before = context.get_account(context.reserve_address).await;

    // Merging two activated stake accounts should succeed.
    context.merge_stake(&validator, 0, 1).await;

    let solido_after = context.get_solido().await;
    let mut reserve_after = context.get_account(context.reserve_address).await;

    let sol_before = Lido::get_sol_balance(
        solido_before.validators.entries.iter(),
        &rent,
        &get_account_info(&context.reserve_address, &mut reserve_before),
    );
    let sol_after = Lido::get_sol_balance(
        solido_after.validators.entries.iter(),
        &rent,
        &get_account_info(&context.reserve_address, &mut reserve_after),
    );
    assert_eq!(
        sol_before, sol_after,
        "Merging should not change the total amount of SOL."
    );
}

#[tokio::test]
async fn test_merge_validator_with_zero_and_one_stake_account() {
    let mut context = Context::new_with_maintainer().await;
    context.add_validator().await;
    let validator = &context.get_solido().await.validators.entries[0];
    context.deposit(Lamports(10_000_000_000)).await;

    // Try to merge stake on a validator that has no stake accounts.
    let result = context.try_merge_stake(&validator, 0, 1).await;
    assert_solido_error!(result, LidoError::InvalidStakeAccount);

    context
        .stake_deposit(
            *validator.pubkey(),
            StakeDeposit::Append,
            Lamports(10_000_000_000),
        )
        .await;

    // Try to merge stake on a validator that has 1 stake account.
    let result = context.try_merge_stake(&validator, 0, 1).await;
    assert_solido_error!(result, LidoError::InvalidStakeAccount);
}

#[tokio::test]
async fn test_merge_with_donated_stake() {
    let (mut context, _stake_account_pubkeys) = Context::new_with_two_stake_accounts().await;
    let validator = &context.get_solido().await.validators.entries[0];
    let (from_stake_account, _) = validator.find_stake_account_address(
        &solido_context::id(),
        &context.solido.pubkey(),
        0,
        StakeType::Stake,
    );
    context
        .fund(from_stake_account, Lamports(100_000_000_000))
        .await;

    let to_account = context.merge_stake(&validator, 0, 1).await;
    let to_balance = context.get_sol_balance(to_account).await;

    assert_eq!(
        to_balance,
        // The initial two accounts had 10 SOL each, and we added a donation of 100.
        Lamports(120_000_000_000),
    );
}
