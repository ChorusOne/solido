// SPDX-FileCopyrightText: 2022 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use anker::{
    error::AnkerError,
    state::{HistoricalStSolPrice, POOL_PRICE_MIN_SAMPLE_DISTANCE, POOL_PRICE_NUM_SAMPLES},
    token::MicroUst,
};
use lido::token::Lamports;
use solana_program::clock::DEFAULT_SLOTS_PER_EPOCH;
use solana_program_test::tokio;
use testlib::{anker_context::Context, assert_solido_error};

const DEPOSIT_AMOUNT: u64 = 1_000_000_000; // 1e9 units

#[tokio::test]
async fn test_successful_fetch_pool_price() {
    let mut context = Context::new().await;
    context
        .initialize_token_pool_and_deposit(Lamports(DEPOSIT_AMOUNT))
        .await;
    for epoch in 2..2 + POOL_PRICE_NUM_SAMPLES as u64 {
        context.solido_context.advance_to_normal_epoch(epoch);
        context.fetch_pool_price().await;
    }
    let anker = context.get_anker().await;
    let current_ust_price = MicroUst(909090909);
    let mut expected_historical_st_sol_prices = (0..5)
        .map(|i| HistoricalStSolPrice {
            slot: 1388256 + i * DEFAULT_SLOTS_PER_EPOCH,
            st_sol_price_in_ust: current_ust_price,
        })
        .collect::<Vec<HistoricalStSolPrice>>();

    assert_eq!(
        anker.historical_st_sol_prices.0[..],
        expected_historical_st_sol_prices
    );

    expected_historical_st_sol_prices.rotate_left(1);
    expected_historical_st_sol_prices[POOL_PRICE_NUM_SAMPLES - 1] = HistoricalStSolPrice {
        slot: 3548256,
        st_sol_price_in_ust: MicroUst(909090909),
    };
    context
        .solido_context
        .advance_to_normal_epoch(2 + POOL_PRICE_NUM_SAMPLES as u64);
    context.fetch_pool_price().await;
    let anker = context.get_anker().await;
    assert_eq!(
        anker.historical_st_sol_prices.0[..],
        expected_historical_st_sol_prices
    );
}

#[tokio::test]
async fn test_fail_fetch_pool_price_too_early() {
    let mut context = Context::new().await;
    context
        .initialize_token_pool_and_deposit(Lamports(DEPOSIT_AMOUNT))
        .await;
    context.fetch_pool_price().await;
    context
        .solido_context
        .context
        .warp_to_slot(956256 + POOL_PRICE_MIN_SAMPLE_DISTANCE - 1)
        .expect("Failed to warp to slot");
    let result = context.try_fetch_pool_price().await;
    assert_solido_error!(result, AnkerError::FetchPoolPriceTooEarly);
}
