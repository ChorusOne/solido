// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use anker::token::BLamports;
use anker::error::AnkerError;
use lido::token::{Lamports, StLamports};
use solana_program_test::tokio;
use solana_sdk::signer::Signer;
use testlib::anker_context::Context;
use testlib::assert_solido_error;

const TEST_DEPOSIT_AMOUNT: StLamports = StLamports(1_000_000_000);

#[tokio::test]
async fn test_successful_deposit() {
    let mut context = Context::new().await;
    let (_owner, recipient) = context.deposit(Lamports(TEST_DEPOSIT_AMOUNT.0)).await;

    let reserve_balance = context
        .solido_context
        .get_st_sol_balance(context.reserve)
        .await;
    let recipient_balance = context.get_b_sol_balance(recipient).await;

    // The context starts Solido with 1:1 exchange rate.
    assert_eq!(reserve_balance, TEST_DEPOSIT_AMOUNT);
    assert_eq!(recipient_balance, BLamports(TEST_DEPOSIT_AMOUNT.0));
}

#[tokio::test]
async fn test_successful_deposit_different_exchange_rate() {
    let mut context = Context::new_different_exchange_rate().await;
    let (_owner, recipient) = context.deposit(Lamports(TEST_DEPOSIT_AMOUNT.0)).await;
    let reserve_balance = context
        .solido_context
        .get_st_sol_balance(context.reserve)
        .await;
    let recipient_balance = context.get_b_sol_balance(recipient).await;

    // The exchange rate is now 1:2.
    assert_eq!(reserve_balance, StLamports(500_000_000));
    assert_eq!(recipient_balance, BLamports(TEST_DEPOSIT_AMOUNT.0));
}

#[tokio::test]
async fn test_deposit_fails_with_wrong_reserve() {
    let mut context = Context::new().await;

    let fake_reserve = context.solido_context.deterministic_keypair.new_keypair();
    context.reserve = fake_reserve.pubkey();

    // The program should confirm that the reserve we use is the reserve of the
    // instance, and fail the transaction if it's a different account. Otherwise
    // we could pass in a reserve controlled by us (where we are an attacker), and
    // get bSOL while also retaining the stSOL.
    let result = context.try_deposit(Lamports(TEST_DEPOSIT_AMOUNT.0)).await;
    assert_solido_error!(result, AnkerError::InvalidDerivedAccount);
}
