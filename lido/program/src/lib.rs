#[cfg(not(feature = "no-entrypoint"))]
pub mod entrypoint;
pub mod error;
pub mod instruction;
pub mod processor;
pub mod state;
pub (crate) mod logic;

/// Seed for reserve authority in SOL
pub const RESERVE_AUTHORITY_ID: &[u8] = b"reserve_authority";
/// Seed for deposit authority
pub const DEPOSIT_AUTHORITY_ID: &[u8] = b"deposit_authority";
/// Seed for token reserve authority
pub const STAKE_POOL_TOKEN_RESERVE_AUTHORITY_ID: &[u8] = b"token_reserve_authority";

solana_program::declare_id!("LidoB9L9nR3CrcaziKVYVpRX6A9Y1LAXYasjjfCbApj");
