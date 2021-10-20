// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use lido::token::Lamports;
use solana_program_test::tokio;
use testlib::anker_context::Context;

#[tokio::test]
async fn test_successful_deposit() {
    let mut _context = Context::new().await;

    let _amount = Lamports(1_000_000_000);
    // let (_owner, _recipient) = context.deposit(amount).await;

    // TODO(#449, ruuda): Finish deposit test.
}
