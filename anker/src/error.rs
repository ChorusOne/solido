use std::fmt::Formatter;

use num_derive::FromPrimitive;
use solana_program::{decode_error::DecodeError, program_error::ProgramError};

/// Errors that may be returned by the Anker program.
#[derive(Clone, Debug, Eq, FromPrimitive, PartialEq)]
pub enum AnkerError {
    /// We failed to deserialize an SPL token account.
    InvalidTokenAccount = 0,

    /// We expected the SPL token account to be owned by the SPL token program.
    InvalidTokenAccountOwner = 1,

    /// The mint of a provided SPL token account does not match the expected mint.
    InvalidTokenMint = 2,

    /// The provided reserve is invalid.
    InvalidReserveAccount = 3,

    /// The provided Solido state is different from the stored one.
    InvalidSolidoInstance = 4,

    /// The one of the provided accounts does not match the expected derived address.
    InvalidDerivedAccount = 5,

    /// An account is not owned by the expected owner.
    InvalidOwner = 6,

    /// Wrong SPL Token Swap instance.
    WrongSplTokenSwap = 7,

    /// Wrong parameters for the SPL Token Swap instruction.
    WrongSplTokenSwapParameters = 8,

    /// The provided rewards destination is different from what is stored in the instance.
    InvalidRewardsDestination = 9,

    /// The amount of rewards to be claimed are zero.
    ZeroRewardsToClaim = 10,
}

// Just reuse the generated Debug impl for Display. It shows the variant names.
impl std::fmt::Display for AnkerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

impl From<AnkerError> for ProgramError {
    fn from(e: AnkerError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

impl<T> DecodeError<T> for AnkerError {
    fn type_of() -> &'static str {
        "Anker Error"
    }
}
