// SPDX-FileCopyrightText: 2022 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use anker::{
    error::AnkerError,
    state::{HistoricalStSolPrice, POOL_PRICE_MIN_SAMPLE_DISTANCE, POOL_PRICE_NUM_SAMPLES},
    token::MicroUst,
};
use lido::token::{Lamports, StLamports};
use solana_program::clock::DEFAULT_SLOTS_PER_EPOCH;
use solana_program_test::tokio;
use solana_sdk::signer::Signer;
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

    // Initially, there are 10 stSOL and 10_000 UST in the pool.
    // for maintaining the constant product k = 10 * 10_000 = 100_000.
    // When selling 1 StSOL we should maintain the equality:
    // (10 + 1) * (10_000 - x) = k, x = 909.0909090909091
    let current_ust_price = MicroUst(909_090_909);

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
async fn test_fetch_pool_price_when_price_changed() {
    let mut context = Context::new().await;
    context
        .initialize_token_pool_and_deposit(Lamports(DEPOSIT_AMOUNT))
        .await;

    // Deposit some tokens so we have StSol
    let (st_sol_keypair, st_sol_token) = context
        .solido_context
        .deposit(Lamports(10_000_000_000))
        .await;
    let ust_address = context
        .create_ust_token_account(st_sol_keypair.pubkey())
        .await;

    context.solido_context.advance_to_normal_epoch(2);
    context.fetch_pool_price().await;

    let amount_in = StLamports(1_000_000_000);
    let min_amount_out = MicroUst(0);
    context
        .swap_st_sol_for_ust(
            &st_sol_token,
            &ust_address,
            &st_sol_keypair,
            amount_in,
            min_amount_out,
        )
        .await;

    context.solido_context.advance_to_normal_epoch(3);
    context.fetch_pool_price().await;

    let anker = context.get_anker().await;
    assert_eq!(
        anker.historical_st_sol_prices.0[POOL_PRICE_NUM_SAMPLES - 2],
        HistoricalStSolPrice {
            slot: 1388256,
            st_sol_price_in_ust: MicroUst(909_090_909)
        }
    );

    // There are 11 stSOL and 9090_909_091 UST in the pool.
    // for maintaining the constant product k = 11 * 9090.909_091 = 100_000.
    // When selling 1 StSOL we should maintain the equality:
    // (11 + 1) * (9090.909091 - x) = k, x = 757.5757576666656

    assert_eq!(
        anker.historical_st_sol_prices.0[POOL_PRICE_NUM_SAMPLES - 1],
        HistoricalStSolPrice {
            slot: 1820256,
            st_sol_price_in_ust: MicroUst(757_575_757)
        }
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
    context
        .solido_context
        .context
        .warp_to_slot(956256 + POOL_PRICE_MIN_SAMPLE_DISTANCE + 1)
        .expect("Failed to warp to slot");
    context.fetch_pool_price().await;
}
