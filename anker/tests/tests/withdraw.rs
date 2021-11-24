// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use anker::error::AnkerError;
use anker::token::BLamports;
use lido::token::{Lamports, StLamports};
use solana_program_test::tokio;
use testlib::anker_context::Context;
use testlib::assert_solido_error;

const TEST_DEPOSIT_AMOUNT: Lamports = Lamports(1_000_000_000);

#[tokio::test]
async fn test_withdraw_single_epoch() {
    let mut context = Context::new().await;

    // Deposit some SOL into Solido, then put that in Anker.
    let (owner, b_sol_recipient) = context.deposit(TEST_DEPOSIT_AMOUNT).await;
    let b_sol_balance = context.get_b_sol_balance(b_sol_recipient).await;

    // We now own all bSOL in existence.
    let b_sol_supply = context.get_b_sol_supply().await;
    assert_eq!(b_sol_balance, b_sol_supply);

    // Withdraw the full amount from Anker again.
    let st_sol_recipient = context
        .withdraw(&owner, b_sol_recipient, b_sol_balance)
        .await;

    // After withdrawing, no bSOL exists any more.
    let b_sol_supply = context.get_b_sol_supply().await;
    assert_eq!(b_sol_supply, BLamports(0));

    // The SOL value of that stSOL should be the same as what we put in.
    let st_sol_balance = context
        .solido_context
        .get_st_sol_balance(st_sol_recipient)
        .await;
    let sol_value = context.exchange_st_sol(st_sol_balance).await;
    assert_eq!(sol_value, TEST_DEPOSIT_AMOUNT);

    // The reserve should now be empty, we withdrew everything.
    let reserve_st_sol = context
        .solido_context
        .get_st_sol_balance(context.reserve)
        .await;
    assert_eq!(reserve_st_sol, StLamports(0));
}

#[tokio::test]
async fn test_withdraw_after_st_sol_price_increase() {
    let mut context = Context::new().await;

    // Deposit some SOL into Solido, then put that in Anker.
    let (owner, b_sol_recipient) = context.deposit(TEST_DEPOSIT_AMOUNT).await;
    let b_sol_balance = context.get_b_sol_balance(b_sol_recipient).await;

    // Donate some SOL to Solido to simulate rewards, and warp to the next epoch
    // to pick up the new exchange rate.
    let donation = Lamports(1_000_000_000);
    context
        .solido_context
        .fund(context.solido_context.reserve_address, donation)
        .await;
    context.solido_context.advance_to_normal_epoch(1);
    context.solido_context.update_exchange_rate().await;

    // We now own all bSOL in existence.
    let b_sol_supply = context.get_b_sol_supply().await;
    assert_eq!(b_sol_balance, b_sol_supply);

    // Withdraw the full amount from Anker again.
    let st_sol_recipient = context
        .withdraw(&owner, b_sol_recipient, b_sol_balance)
        .await;

    // After withdrawing, no bSOL exists any more.
    let b_sol_supply = context.get_b_sol_supply().await;
    assert_eq!(b_sol_supply, BLamports(0));

    // The SOL value of that stSOL should be the same as what we put in.
    // One lamport is lost due to rounding errors though.
    let st_sol_balance = context
        .solido_context
        .get_st_sol_balance(st_sol_recipient)
        .await;
    let sol_value = context.exchange_st_sol(st_sol_balance).await;
    assert_eq!(sol_value, (TEST_DEPOSIT_AMOUNT - Lamports(1)).unwrap());

    // Some stSOL should be left in the reserve: these are the staking rewards
    // that the bSOL holder renounced by converting their stSOL into bSOL. Half
    // of the stSOL is held by the initial Solido depositor set up by the test
    // context, the other half of the stSOL was held by Anker at the time of the
    // donation. So now the value of the reserve should be half the deposit.
    // (Plus one lamport rounding error.)
    let reserve_st_sol = context
        .solido_context
        .get_st_sol_balance(context.reserve)
        .await;
    let reserve_sol = context.exchange_st_sol(reserve_st_sol).await;
    assert_eq!(reserve_sol, Lamports(500_000_001));
}

#[tokio::test]
async fn test_withdraw_wrong_token_mint() {
    let mut context = Context::new().await;

    let (owner, st_sol_account) = context
        .solido_context
        .deposit(Lamports(1_000_000_000))
        .await;
    let b_sol_account = context
        .try_deposit_st_sol(&owner, st_sol_account, StLamports(500_000_000))
        .await
        .unwrap();

    // Withdrawing with the wrong type of account should fail. We need to put in
    // a bSOL account, not an stSOL account to withdraw from.
    let result = context
        .try_withdraw(&owner, st_sol_account, BLamports(250_000_000))
        .await;
    assert_solido_error!(result, AnkerError::InvalidTokenMint);

    // With the right type of account, it should succeed.
    context
        .withdraw(&owner, b_sol_account, BLamports(250_000_000))
        .await;
}
