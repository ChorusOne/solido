// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use testlib::anker_context::Context;
use testlib::assert_solido_error;

use anchor_integration::token::BLamports;
use lido::token::Lamports;

#[tokio::test]
async fn test_successful_deposit() {
    let mut context = Context::new();

    let (_, recipient) = context.deposit(TEST_DEPOSIT_AMOUNT).await;

    let reserve_balance = context.get_sol_balance(context.reserve_address).await;
    let rent = context.get_rent().await;
    assert_eq!(
        Ok(reserve_balance),
        TEST_DEPOSIT_AMOUNT + Lamports(rent.minimum_balance(0))
    );
}
