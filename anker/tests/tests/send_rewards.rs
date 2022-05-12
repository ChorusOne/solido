// SPDX-FileCopyrightText: 2022 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use solana_program::instruction::InstructionError;
use std::mem;

use solana_program::pubkey::Pubkey;
use solana_program_test::tokio;
use solana_sdk::transaction::TransactionError;
use solana_sdk::transport::TransportError;

use anker::{
    error::AnkerError,
    state::{POOL_PRICE_MIN_SAMPLE_DISTANCE, POOL_PRICE_NUM_SAMPLES},
    token::MicroUst,
};
use lido::token::{Lamports, StLamports};
use testlib::{anker_context::Context, assert_solido_error};

const DEPOSIT_AMOUNT: u64 = 1_000_000_000; // 1e9 units

#[tokio::test]
async fn test_send_rewards_does_not_overflow_stack() {
    let mut context = Context::new().await;
    context
        .initialize_token_pool_and_deposit(Lamports(DEPOSIT_AMOUNT))
        .await;
    context.fill_historical_st_sol_price_array().await;
    context.sell_rewards().await;

    let result = context.try_send_rewards().await;

    match result {
        Err(TransportError::TransactionError(TransactionError::InstructionError(
            0,
            InstructionError::ProgramFailedToComplete,
        ))) => panic!("Did the program overflow the stack?"),
        Ok(()) => panic!("This should not have passed without the Wormhole program present."),
        Err(err) => {
            println!("Unexpected error: {:?}", err);
        } // TODO: Add a case for the expected error after resolving the stack overflow.
    }
}
