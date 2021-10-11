use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use lido::util::serialize_b58;
use serde::Serialize;
use solana_program::{account_info::AccountInfo, entrypoint::ProgramResult, pubkey::Pubkey};

#[repr(C)]
#[derive(
    Clone, Debug, Default, BorshDeserialize, BorshSerialize, BorshSchema, Eq, PartialEq, Serialize,
)]
pub struct Anchor {
    /// The SPL Token mint address for bSOL.
    #[serde(serialize_with = "serialize_b58")]
    pub bsol_mint: Pubkey,

    /// The associated LIDO state address.
    #[serde(serialize_with = "serialize_b58")]
    pub lido: Pubkey,

    /// Bump seeds for signing messages on behalf of the authority.
    pub mint_authority_bump_seed: u8,
}

impl Anchor {
    pub fn save(&self, account: &AccountInfo) -> ProgramResult {
        // NOTE: If you ended up here because the tests are failing because the
        // runtime complained that an account's size was modified by a program
        // that wasn't its owner, double check that the name passed to
        // ProgramTest matches the name of the crate.
        BorshSerialize::serialize(self, &mut *account.data.borrow_mut())?;
        Ok(())
    }
}
