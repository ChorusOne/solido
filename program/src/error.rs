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
    /// Lido account mismatch the one stored in the Lido program
    #[error("InvalidOwner")]
    InvalidOwner,
    /// Stake pool is in an invalid state
    #[error("WrongStakePool")]
    InvalidStakePool,
    /// Invalid allocated amount
    #[error("InvalidAmount")]
    InvalidAmount,
    /// A required signature is missing
    #[error("SignatureMissing")]
    SignatureMissing,
    // 5.
    /// The token minter is different from Lido's
    #[error("InvalidTokenMinter")]
    InvalidTokenMinter,
    /// The reserve authority is invalid
    #[error("InvalidReserveAuthority")]
    InvalidReserveAuthority,
    /// Calculation failed due to division by zero or overflow
    #[error("CalculationFailure")]
    CalculationFailure,
    #[error("InvalidStaker")]
    /// Invalid manager
    InvalidStaker,
    #[error("WrongStakeState")]
    /// Stake account does not exist or is in an invalid state
    WrongStakeState,
    // 10.
    /// Token program
    #[error("InvalidTokenProgram")]
    /// Token token program ID is different from Lido's
    InvalidTokenProgram,
    #[error("InvalidPoolToken")]
    /// Owner of the Stake pool token is invalid
    InvalidPoolToken,
    /// The sum of numerators should be equal to the denominators
    #[error("InvalidFeeAmount")]
    InvalidFeeAmount,
    /// Number of maximum validators reached
    #[error("InvalidFeeAmount")]
    MaximumValidatorsExceeded,
    /// An invalid validator credit account size was supplied
    #[error("UnexpectedValidatorCreditAccountSize")]
    UnexpectedValidatorCreditAccountSize,
    // 15
    /// Wrong manager trying  to alter the state
    #[error("InvalidManager")]
    InvalidManager,
    /// One of the provided accounts had a mismatch in is_writable or is_signer.
    #[error("InvalidAccountInfo")]
    InvalidAccountInfo,
    /// More accounts were provided than the program expects.
    #[error("TooManyAccountKeys")]
    TooManyAccountKeys,
    /// Wrong fee distribution account
    #[error("InvalidFeeDistributionAccount")]
    InvalidFeeDistributionAccount,
    /// Wrong validator credits account
    #[error("InvalidValidatorCreditAccount")]
    InvalidValidatorCreditAccount,
    // 20
    /// Validator credit account was changed
    #[error("ValidatorCreditChanged")]
    ValidatorCreditChanged,
    /// Fee account should be the same as the Stake pool fee'
    #[error("ValidatorCreditChanged")]
    InvalidFeeAccount,
    /// One of the fee recipients is invalid
    #[error("InvalidFeeRecipient")]
    InvalidFeeRecipient,
    /// There is a stake account with the same key present in the validator
    /// credit list.
    #[error("DuplicatedValidatorCreditStakeAccount")]
    DuplicatedValidatorCreditStakeAccount,
    /// Validator credit account was not found
    #[error("ValidatorCreditNotFound")]
    ValidatorCreditNotFound,
    // 25
    /// Validator has unclaimed credit, should mint the tokens before the validator removal
    #[error("ValidatorHasUnclaimedCredit")]
    ValidatorHasUnclaimedCredit,
    /// The reserve account is not rent exempt
    #[error("ReserveIsNotRentExempt")]
    ReserveIsNotRentExempt,
    /// The requested amount for reserve withdrawal exceeds the maximum held in
    /// the reserve account considering rent exemption
    #[error("AmountExceedsReserve")]
    AmountExceedsReserve,
    /// Number of maximum maintainers reached
    #[error("MaximumMaintainersExceeded")]
    MaximumMaintainersExceeded,
    /// The same maintainer's public key already exists in the structure
    #[error("DuplicatedMaintainer")]
    DuplicatedMaintainer,
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
