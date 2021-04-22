#[cfg(not(feature = "no-entrypoint"))]
pub mod entrypoint;
pub mod error;
pub mod instruction;
pub mod processor;
pub mod state;

/// Seed for lido authority authority seed
pub const AUTHORITY_ID: &[u8] = b"lido_authority";

solana_program::declare_id!("LidoB9L9nR3CrcaziKVYVpRX6A9Y1LAXYasjjfCbApj");
