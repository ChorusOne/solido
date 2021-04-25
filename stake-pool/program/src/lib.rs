// #![deny(missing_docs)]

//! A program for creating and managing pools of stake

pub mod borsh;
pub mod error;
pub mod instruction;
pub mod processor;
pub mod stake_program;
pub mod state;

#[cfg(not(feature = "no-entrypoint"))]
pub mod entrypoint;

// Export current sdk types for downstream users building with a different sdk version
pub use solana_program;
use {
    crate::stake_program::Meta,
    solana_program::{native_token::LAMPORTS_PER_SOL, pubkey::Pubkey},
};

/// Seed for deposit authority seed
const AUTHORITY_DEPOSIT: &[u8] = b"deposit";

/// Seed for withdraw authority seed
const AUTHORITY_WITHDRAW: &[u8] = b"withdraw";

/// Seed for transient stake account
const TRANSIENT_STAKE_SEED: &[u8] = b"transient";

/// Minimum amount of staked SOL required in a validator stake account to allow
/// for merges without a mismatch on credits observed
pub const MINIMUM_ACTIVE_STAKE: u64 = LAMPORTS_PER_SOL;

/// Maximum amount of validator stake accounts to update per
/// `UpdateValidatorListBalance` instruction, based on compute limits
pub const MAX_VALIDATORS_TO_UPDATE: usize = 10;

/// Get the stake amount under consideration when calculating pool token
/// conversions
pub fn minimum_stake_lamports(meta: &Meta) -> u64 {
    meta.rent_exempt_reserve
        .saturating_add(MINIMUM_ACTIVE_STAKE)
}

/// Get the stake amount under consideration when calculating pool token
/// conversions
pub fn minimum_reserve_lamports(meta: &Meta) -> u64 {
    meta.rent_exempt_reserve.saturating_add(1)
}

/// Generates the deposit authority program address for the stake pool
pub fn find_deposit_authority_program_address(
    program_id: &Pubkey,
    stake_pool_address: &Pubkey,
) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[&stake_pool_address.to_bytes()[..32], AUTHORITY_DEPOSIT],
        program_id,
    )
}

/// Generates the withdraw authority program address for the stake pool
pub fn find_withdraw_authority_program_address(
    program_id: &Pubkey,
    stake_pool_address: &Pubkey,
) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[&stake_pool_address.to_bytes()[..32], AUTHORITY_WITHDRAW],
        program_id,
    )
}

/// Generates the stake program address for a validator's vote account
pub fn find_stake_program_address(
    program_id: &Pubkey,
    vote_account_address: &Pubkey,
    stake_pool_address: &Pubkey,
) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            &vote_account_address.to_bytes()[..32],
            &stake_pool_address.to_bytes()[..32],
        ],
        program_id,
    )
}

/// Generates the stake program address for a validator's vote account
pub fn find_transient_stake_program_address(
    program_id: &Pubkey,
    vote_account_address: &Pubkey,
    stake_pool_address: &Pubkey,
) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            TRANSIENT_STAKE_SEED,
            &vote_account_address.to_bytes()[..32],
            &stake_pool_address.to_bytes()[..32],
        ],
        program_id,
    )
}

solana_program::declare_id!("poo1B9L9nR3CrcaziKVYVpRX6A9Y1LAXYasjjfCbApj");
