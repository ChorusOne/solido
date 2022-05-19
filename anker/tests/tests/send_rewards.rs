// SPDX-FileCopyrightText: 2022 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use solana_program::instruction::InstructionError;
use solana_program_test::tokio;
use solana_sdk::transaction::TransactionError;
use solana_sdk::transport::TransportError;

use lido::token::Lamports;
use testlib::anker_context::Context;

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
        // An access violation caused by a stack overflow results in this error.
        Err(TransportError::TransactionError(TransactionError::InstructionError(
            0,
            InstructionError::ProgramFailedToComplete,
        ))) => {
            panic!("Did the program overflow the stack?")
        }
        Err(TransportError::TransactionError(TransactionError::InstructionError(
            0,
            InstructionError::AccountNotExecutable,
        ))) => {
            // This error is expected, we try to call a dummy address where the
            // Wormhole program is supposed to live, but that dummy address is
            // not executable. If we get here, it means we executed the entire
            // `SendRewards` instruction aside from the final call to the
            // Wormhole progrma. In particular we know that we didn't overflow
            // the stack.
        }
        Ok(()) => panic!("This should not have passed without the Wormhole program present."),
        Err(err) => {
            panic!("Unexpected error: {:?}", err);
        }
    }
}
