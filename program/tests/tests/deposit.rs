// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use testlib::assert_solido_error;
use testlib::solido_context::Context;

use lido::error::LidoError;
use lido::token::Lamports;
use solana_program_test::tokio;
use solana_sdk::signer::Signer;

pub const TEST_DEPOSIT_AMOUNT: Lamports = Lamports(100_000_000);

#[tokio::test]
async fn test_successful_deposit() {
    let mut context = Context::new_with_maintainer_and_validator().await;

    let (_, recipient) = context.deposit(TEST_DEPOSIT_AMOUNT).await;

    let reserve_balance = context.get_sol_balance(context.reserve_address).await;
    let rent = context.get_rent().await;
    assert_eq!(
        Ok(reserve_balance),
        TEST_DEPOSIT_AMOUNT + Lamports(rent.minimum_balance(0))
    );

    // In general, the received stSOL need not be equal to the deposited SOL,
    // but initially, the exchange rate is 1, so this holds.
    let st_sol_balance = context.get_st_sol_balance(recipient).await;
    assert_eq!(st_sol_balance.0, TEST_DEPOSIT_AMOUNT.0);

    let solido = context.get_solido().await.lido;
    assert_eq!(solido.metrics.deposit_amount.total, TEST_DEPOSIT_AMOUNT);
    assert_eq!(solido.metrics.deposit_amount.num_observations(), 1);
}

/// This is a regression test for a vulnerability that allowed anybody to pass in
/// a different reserve account than the one owned by Solido when doing a deposit.
#[tokio::test]
async fn test_deposit_fails_with_wrong_reserve() {
    let mut context = Context::new_with_maintainer().await;

    let fake_reserve = context.deterministic_keypair.new_keypair();
    context.reserve_address = fake_reserve.pubkey();

    // Try to deposit, but this now uses our fake reserve address in the instruction.
    // If this does not fail, an attacker can pass in an account controlled by
    // themselves as the reserve, and keep the SOL but get the stSOL as well.
    let result = context.try_deposit(TEST_DEPOSIT_AMOUNT).await;

    assert_solido_error!(result, LidoError::InvalidReserveAccount);
}
