// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

#[cfg(not(feature = "no-entrypoint"))]
pub mod entrypoint;
mod instruction;
mod logic;
mod processor;
mod state;
mod token;

/// Mint authority, mints StSol.
pub const ANCHOR_MINT_AUTHORITY: &[u8] = b"mint_authority";
