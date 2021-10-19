// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

//! Test context for testing the Anchor integration.

use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signer;

use crate::solido_context;
use anchor_integration;

// Program id for the Anchor integration program. Only used for tests.
solana_program::declare_id!("AnchwRMMkz4t63Rr8P6m7mx6qBHetm8yZ4xbeoDSAeQZ");

pub struct Context {
    solido_context: solido_context::Context,
    anker: Pubkey,
}

impl Context {
    pub async fn new_empty() -> Context {
        let solido_context = solido_context::Context::new_with_maintainer().await;
        let (anker, _seed) =
            anchor_integration::find_instance_address(&id(), &solido_context.solido.pubkey());
        Context {
            solido_context,
            anker,
        }
    }
}
