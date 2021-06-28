use solana_program::pubkey::Pubkey;

#[cfg(not(feature = "no-entrypoint"))]
pub mod entrypoint;

pub mod accounts;
pub mod account_map;
pub mod balance;
pub mod error;
pub mod instruction;
pub(crate) mod logic;
pub(crate) mod process_management;
pub mod processor;
pub mod state;
pub mod token;
pub mod util;

/// Seed for reserve authority in SOL.
pub const RESERVE_AUTHORITY: &[u8] = b"reserve_authority";

/// Seed for managing the stake.
pub const STAKE_AUTHORITY: &[u8] = b"stake_authority";

/// Additional seed for validator stake accounts.
pub const VALIDATOR_STAKE_ACCOUNT: &[u8] = b"validator_stake_account";

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

/// The minimum amount to put in a stake account (1 SOL).
///
/// For stake accounts, there is a minimum balance for the account to be
/// rent-exempt, that depends on the size of the stake program's stake state
/// struct. But aside from this minimum, in order to merge two stake accounts,
/// their `credits_observed` must match. If the rewards received is less than a
/// single Lamport, then `credits_observed` will not be updated, and then the
/// stake account cannot be merged into a different stake account. Because we
/// need to be able to merge stake accounts, we also need to make sure that they
/// contain enough stake that they will earn at least one lamport per epoch.
/// 1 SOL should be sufficient for that.
pub const MINIMUM_STAKE_ACCOUNT_BALANCE: token::Lamports = token::Lamports(1_000_000_000);

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn minimum_stake_account_balance_is_at_least_rent_exempt() {
        use crate::token::Lamports;
        use solana_program::rent::Rent;
        use spl_stake_pool::stake_program::StakeState;

        let rent = Rent::default();
        let minimum_rent_exempt_balance =
            Lamports(rent.minimum_balance(std::mem::size_of::<StakeState>()));

        // Sanity check that the default rent instance is not for free. In theory
        // the rent could change dynamically on the network, but in practice,
        // it has been hard-coded since forever, and it is unlikely to suddenly
        // change, because half the Solana ecosystem would break.
        assert!(minimum_rent_exempt_balance > Lamports(0));
        assert!(MINIMUM_STAKE_ACCOUNT_BALANCE > minimum_rent_exempt_balance);
    }
}
