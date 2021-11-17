// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use lido::token::Lamports;
use solana_program_test::tokio;
use testlib::anker_context::Context;

const TEST_DEPOSIT_AMOUNT: Lamports = Lamports(1_000_000_000);

#[tokio::test]
async fn test_deposit_withdraw_single_epoch() {
    let mut context = Context::new().await;

    // Deposit some SOL into Solido, then put that in Anker.
    let (owner, b_sol_recipient) = context.deposit(TEST_DEPOSIT_AMOUNT).await;

    // Withdraw the full amount from Anker again.
    let b_sol_balance = context.get_b_sol_balance(b_sol_recipient).await;
    let st_sol_recipient = context
        .withdraw(&owner, b_sol_recipient, b_sol_balance)
        .await;

    // Then compute how much that is worth in SOL.
    let solido = context.solido_context.get_solido().await;
    let st_sol_balance = context
        .solido_context
        .get_st_sol_balance(st_sol_recipient)
        .await;
    let sol_value = solido
        .exchange_rate
        .exchange_st_sol(st_sol_balance)
        .unwrap();

    assert_eq!(sol_value, TEST_DEPOSIT_AMOUNT);
}
