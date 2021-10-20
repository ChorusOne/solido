use std::fmt::Formatter;

use num_derive::FromPrimitive;
use solana_program::{decode_error::DecodeError, program_error::ProgramError};

/// Errors that may be returned by the Lido program.
#[derive(Clone, Debug, Eq, FromPrimitive, PartialEq)]
pub enum AnchorError {
    /// We expected an SPL token account that holds bSOL,
    /// but this was not an SPL token account,
    /// or its mint did not match.
    InvalidBSolAccount = 0,

    /// We expected the BSol account to be owned by the SPL token program.
    InvalidBSolAccountOwner = 1,

    /// The provided mint is invalid.
    InvalidBSolMint = 2,

    /// The provided reserve is invalid.
    InvalidReserveAccount = 3,

    /// The provided Lido state is different from the stored one.
    WrongLidoInstance = 4,
}

// Just reuse the generated Debug impl for Display. It shows the variant names.
impl std::fmt::Display for AnchorError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

impl From<AnchorError> for ProgramError {
    fn from(e: AnchorError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

impl<T> DecodeError<T> for AnchorError {
    fn type_of() -> &'static str {
        "Lido Error"
    }
}
