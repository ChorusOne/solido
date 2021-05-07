//! Error types

use num_derive::FromPrimitive;
use solana_program::{decode_error::DecodeError, program_error::ProgramError};
use thiserror::Error;

/// Errors that may be returned by the Lido program.
#[derive(Clone, Debug, Eq, Error, FromPrimitive, PartialEq)]
pub enum LidoError {
    // 0.
    /// Address is already initialized
    #[error("AlreadyInUse")]
    AlreadyInUse,
    /// Lido members account mismatch the one stored in the Lido program
    #[error("InvalidOwner")]
    InvalidOwner,
    /// Invalid stake pool
    #[error("WrongStakePool")]
    InvalidStakePool,
    /// Invalid stake pool
    #[error("InvalidAmount")]
    InvalidAmount,
    /// Invalid stake pool
    #[error("SignatureMissing")]
    SignatureMissing,
    // 5.
    /// Invalid stake pool
    #[error("InvalidToken")]
    InvalidToken,
    #[error("InvalidReserveAuthority")]
    InvalidReserveAuthority,
    #[error("CalculationFailure")]
    CalculationFailure,
    #[error("InvalidStaker")]
    InvalidStaker,
    #[error("WrongStakeState")]
    WrongStakeState,
    // 10.
    #[error("InvalidTokenProgram")]
    InvalidTokenProgram,
    #[error("InvalidPoolToken")]
    InvalidPoolToken,
    /// One of the provided accounts had a mismatch in is_writable or is_signer.
    #[error("IncorrectAccountInto")]
    InvalidAccountInfo,
    /// More accounts were provided than the program expects.
    #[error("TooManyAccountKeys")]
    TooManyAccountKeys,
}
impl From<LidoError> for ProgramError {
    fn from(e: LidoError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
impl<T> DecodeError<T> for LidoError {
    fn type_of() -> &'static str {
        "Lido Error"
    }
}
