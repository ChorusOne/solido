// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use anker::error::AnkerError;
use anker::token::BLamports;
use borsh::BorshSerialize;
use lido::state::Lido;
use lido::token::{Lamports, StLamports};
use solana_program::borsh::try_from_slice_unchecked;
use solana_program_test::tokio;
use solana_sdk::account::WritableAccount;
use solana_sdk::signer::Signer;
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
        .get_st_sol_balance(context.st_sol_reserve)
        .await;
    assert_eq!(reserve_st_sol, StLamports(0));
    let anker = context.get_anker().await;
    assert_eq!(anker.metrics.withdraw_metric.st_sol_total, st_sol_balance);
    assert_eq!(anker.metrics.withdraw_metric.b_sol_total, b_sol_balance);
    assert_eq!(anker.metrics.withdraw_metric.count, 1);
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
        .get_st_sol_balance(context.st_sol_reserve)
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

#[tokio::test]
async fn test_withdraw_after_st_sol_price_decrease() {
    let mut context = Context::new().await;

    // Deposit some SOL into Solido, then put that in Anker.
    let (owner, b_sol_recipient) = context.deposit(TEST_DEPOSIT_AMOUNT).await;
    let b_sol_balance = context.get_b_sol_balance(b_sol_recipient).await;

    // Mutate the Solido instance and sabotage its exchange rate to make the
    // value of stSOL go down. Normally this cannot happen, but if Solana would
    // introduce slashing in the future, then it might.
    context.solido_context.advance_to_normal_epoch(1);
    context.solido_context.update_exchange_rate().await;
    let mut solido_account = context
        .solido_context
        .get_account(context.solido_context.solido.pubkey())
        .await;
    let mut solido = try_from_slice_unchecked::<Lido>(solido_account.data.as_slice()).unwrap();
    // Set 1 stSOL = 0.5 SOL.
    solido.exchange_rate.sol_balance = Lamports(1_000_000_000);
    solido.exchange_rate.st_sol_supply = StLamports(2_000_000_000);
    solido_account.data = BorshSerialize::try_to_vec(&solido).unwrap();
    let mut solido_account_shared = solana_sdk::account::AccountSharedData::new(
        solido_account.lamports,
        solido_account.data.len(),
        &solido_account.owner,
    );
    solido_account_shared.set_rent_epoch(solido_account.rent_epoch);
    solido_account_shared.set_data(solido_account.data);
    context.solido_context.context.set_account(
        &context.solido_context.solido.pubkey(),
        &solido_account_shared,
    );

    assert_eq!(b_sol_balance, BLamports(1_000_000_000));

    // Withdraw 0.1 bSOL.
    let st_sol_recipient = context
        .withdraw(&owner, b_sol_recipient, BLamports(100_000_000))
        .await;

    // We put in 1 SOL, converted it to stSOL, then to bSOL.
    // Then the value of stSOL went down by 50%. This breaks the peg, even though
    // we have 1 bSOL, we can at best withdraw 0.5 SOL now. To make the test
    // more interesting, if we tried to withdraw the full 1 bSOL and we forgot
    // to use the right exchange rate, there is not enough stSOL in existence
    // and the transaction would fail, but if we only withdraw 0.1 bSOL, then
    // if we used the wrong exchange rate, we would get 0.2 stSOL, which we have.
    let st_sol_balance = context
        .solido_context
        .get_st_sol_balance(st_sol_recipient)
        .await;
    // The SOL value of our withdraw is half of the bSOL amount, because the peg
    // is broken.
    let sol_value = context.exchange_st_sol(st_sol_balance).await;
    assert_eq!(sol_value, Lamports(50_000_000));
}
