// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use testlib::anker_context::Context;
use testlib::assert_solido_error;

use anchor_integration::token::BLamports;
use lido::token::Lamports;

#[tokio::test]
async fn test_successful_deposit() {
    let mut context = Context::new();
    let (_owner, _recipient) = context.deposit(TEST_DEPOSIT_AMOUNT).await;

    // TODO(ruuda): Finish deposit test.
}
