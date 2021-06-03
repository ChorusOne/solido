use solana_program::pubkey::Pubkey;

#[cfg(not(feature = "no-entrypoint"))]
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

/// Additional seed for validator stake accounts.
pub const VALIDATOR_STAKE_ACCOUNT: &[u8] = b"validator_stake_account";

pub fn find_authority_program_address(
    program_id: &Pubkey,
    lido_address: &Pubkey,
    authority: &[u8],
) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[&lido_address.to_bytes()[..32], authority], program_id)
}
