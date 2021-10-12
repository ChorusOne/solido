// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use solana_program::pubkey::Pubkey;

#[cfg(not(feature = "no-entrypoint"))]
pub mod entrypoint;
mod instruction;
mod logic;
pub mod processor;
pub mod state;
pub mod token;

/// Mint authority, mints bSOL.
pub const ANCHOR_MINT_AUTHORITY: &[u8] = b"mint_authority";

/// Return the address at which the Anchor instance should live that belongs to
/// the given Solido instance.
pub fn find_instance_address(anker_program_id: &Pubkey, solido_instance: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[solido_instance.as_ref()], anker_program_id)
}
/// Anchor's authority that will control the reserve account.
pub const ANCHOR_RESERVE_AUTHORITY: &[u8] = b"reserve_authority";
