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
    /// Invalid allocated amount
    #[error("InvalidAmount")]
    InvalidAmount,
    /// A required signature is missing
    #[error("SignatureMissing")]
    SignatureMissing,
    /// The reserve authority is invalid
    #[error("InvalidReserveAuthority")]
    InvalidReserveAuthority,
    /// Calculation failed due to division by zero or overflow
    #[error("CalculationFailure")]
    CalculationFailure,
    #[error("WrongStakeState")]
    /// Stake account does not exist or is in an invalid state
    WrongStakeState,
    /// The sum of numerators should be equal to the denominators
    #[error("InvalidFeeAmount")]
    InvalidFeeAmount,
    /// Number of maximum validators reached
    #[error("InvalidFeeAmount")]
    MaximumNumberOfAccountsExceeded,
    /// The size of the account for the Solido state does not match `max_validators`.
    #[error("UnexpectedMaxValidators")]
    UnexpectedMaxValidators,
    /// Wrong manager trying  to alter the state
    #[error("InvalidManager")]
    InvalidManager,
    /// Wrong maintainer trying  to alter the state
    #[error("InvalidMaintainer")]
    InvalidMaintainer,
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
    #[error("DuplicatedEntry")]
    DuplicatedEntry,
    /// Validator credit account was not found
    #[error("ValidatorCreditNotFound")]
    ValidatorCreditNotFound,
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
    /// The same maintainer's public key already exists in the structure
    #[error("DuplicatedMaintainer")]
    DuplicatedMaintainer,
    /// A member of the accounts list (maintainers or validators) is not present
    /// in the structure
    #[error("InvalidAccountMember")]
    InvalidAccountMember,
    /// Lido has an invalid size, calculated with the Lido's constant size plus
    /// required to hold variable structures
    #[error("InvalidAccountMember")]
    InvalidLidoSize,
    /// There are no validators with an active stake account to delegate to.
    #[error("NoActiveValidators")]
    NoActiveValidators,
    /// When staking part of the reserve to a new stake account, the next
    /// program-derived address for the stake account associated with the given
    /// validator, does not match the provided stake account.
    #[error("InvalidStakeAccount")]
    InvalidStakeAccount,

    /// We expected an SPL token account that holds stSOL,
    /// but this was not an SPL token account,
    /// or its mint did not match.
    #[error("InvalidStSolAccount")]
    InvalidStSolAccount,
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
