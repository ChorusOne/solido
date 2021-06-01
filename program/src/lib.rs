use solana_program::pubkey::Pubkey;

#[cfg(not(feature = "no-entrypoint"))]
pub mod balance;
pub mod entrypoint;
pub mod error;
pub mod instruction;
pub(crate) mod logic;
pub(crate) mod process_management;
pub mod processor;
pub mod state;
pub mod token;

/// Seed for reserve authority in SOL
pub const RESERVE_AUTHORITY: &[u8] = b"reserve_authority";
/// Seed for deposit authority
pub const DEPOSIT_AUTHORITY: &[u8] = b"deposit_authority";
/// Seed for fee manager authority
pub const FEE_MANAGER_AUTHORITY: &[u8] = b"fee_authority";
/// Stake pool manager authority
pub const STAKE_POOL_AUTHORITY: &[u8] = b"stake_pool_authority";

/// Finds the public key and bump seed for a given authority.  Since this
/// function can take some time to run, it's preferred to use
/// `Pubkey::create_program_address(seeds, program_id)` inside programs.
pub fn find_authority_program_address(
    program_id: &Pubkey,
    lido_address: &Pubkey,
    authority: &[u8],
) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[&lido_address.to_bytes(), authority], program_id)
}
