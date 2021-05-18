use solana_program::pubkey::Pubkey;

#[cfg(not(feature = "no-entrypoint"))]
pub mod entrypoint;
pub mod error;
pub mod instruction;
pub(crate) mod logic;
pub mod processor;
pub mod state;

/// Seed for reserve authority in SOL
pub const RESERVE_AUTHORITY_ID: &[u8] = b"reserve_authority";
/// Seed for deposit authority
pub const DEPOSIT_AUTHORITY_ID: &[u8] = b"deposit_authority";
/// Seed for token reserve authority
pub const STAKE_POOL_TOKEN_RESERVE_AUTHORITY_ID: &[u8] = b"token_reserve_authority";
/// Seed for fee manafer authority
pub const FEE_MANAGER_AUTHORITY: &[u8] = b"fee_authority";

solana_program::declare_id!("3kEkdGe68DuTKg6FhVrLPZ3Wm8EcUPCPjhCeu8WrGDoc");

pub fn find_authority_program_address(
    program_id: &Pubkey,
    lido_address: &Pubkey,
    authority: &[u8],
) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[&lido_address.to_bytes()[..32], authority], program_id)
}
