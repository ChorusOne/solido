// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use solana_program_test::tokio;

use testlib::assert_solido_error;
use testlib::solido_context::Context;

use lido::error::LidoError;
use lido::state::ExchangeRate;
use lido::token::{Lamports, StLamports};

#[tokio::test]
async fn test_update_exchange_rate() {
    let mut context = Context::new_with_maintainer().await;

    // Move to the next epoch, then update the exchange rate.
    context.advance_to_normal_epoch(0);
    context.update_exchange_rate().await;
    let start_epoch = context.get_clock().await.epoch;

    // Initially the balance is zero, and we haven't minted any stSOL.
    let solido = context.get_solido().await.lido;
    assert_eq!(
        solido.exchange_rate,
        ExchangeRate {
            computed_in_epoch: start_epoch,
            st_sol_supply: StLamports(0),
            sol_balance: Lamports(0),
        }
    );

    // If we try to update once more in this epoch, that should fail.
    let result = context.try_update_exchange_rate().await;
    assert_solido_error!(result, LidoError::ExchangeRateAlreadyUpToDate);

    const DEPOSIT_AMOUNT: u64 = 100_000_000;

    // Make a deposit, so something should change next epoch.
    let (_, recipient) = context.deposit(Lamports(DEPOSIT_AMOUNT)).await;

    // This is the first deposit, so the exchange rate is 1:1, we should have
    // gotten the same number of stSOL lamports, as we put in in SOL lamports.
    let received_st_sol = context.get_st_sol_balance(recipient).await;
    assert_eq!(received_st_sol, StLamports(DEPOSIT_AMOUNT));

    context.advance_to_normal_epoch(1);
    context.update_exchange_rate().await;

    // There was one deposit, the exchange rate was 1:1, we should now have the
    // same amount of SOL and stSOL.
    let solido = context.get_solido().await.lido;
    assert_eq!(
        solido.exchange_rate,
        ExchangeRate {
            computed_in_epoch: start_epoch + 1,
            st_sol_supply: StLamports(DEPOSIT_AMOUNT),
            sol_balance: Lamports(DEPOSIT_AMOUNT),
        }
    );

    // If we make a new deposit, the new exchange rate is used, but it is still 1:1.
    let (_, recipient) = context.deposit(Lamports(DEPOSIT_AMOUNT)).await;
    let received_st_sol = context.get_st_sol_balance(recipient).await;
    assert_eq!(received_st_sol, StLamports(DEPOSIT_AMOUNT));

    // Now donate something to the reserve. This will affect the exchange rate,
    // but only in the next epoch.
    context
        .fund(context.reserve_address, Lamports(3 * DEPOSIT_AMOUNT))
        .await;

    context.advance_to_normal_epoch(2);

    // There is now not as much SOL as stSOL, but for deposits, the rate is still
    // 1:1. Even though we jumped to the next epoch! After all, we did not update
    // the exchange rate yet.
    let (_, recipient) = context.deposit(Lamports(DEPOSIT_AMOUNT)).await;
    let received_st_sol = context.get_st_sol_balance(recipient).await;
    assert_eq!(received_st_sol, StLamports(DEPOSIT_AMOUNT));

    context.update_exchange_rate().await;

    let solido = context.get_solido().await.lido;
    assert_eq!(
        solido.exchange_rate,
        ExchangeRate {
            computed_in_epoch: start_epoch + 2,
            // We had 3 deposits of DEPOSIT_AMOUNT, so stSOL and SOL have at
            // least that. On top, we got a donation of 3 * DEPOSIT_AMOUNT to
            // the reserve.
            st_sol_supply: StLamports(3 * DEPOSIT_AMOUNT),
            sol_balance: Lamports(6 * DEPOSIT_AMOUNT),
        }
    );

    // After the recompute, 1 SOL = 0.5 stSOL.
    let (_, recipient) = context.deposit(Lamports(DEPOSIT_AMOUNT)).await;
    let received_st_sol = context.get_st_sol_balance(recipient).await;
    assert_eq!(received_st_sol, StLamports(DEPOSIT_AMOUNT / 2));
}
