// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

//! Error types

use num_derive::FromPrimitive;
use solana_program::{decode_error::DecodeError, program_error::ProgramError};
use thiserror::Error;

use crate::token::ArithmeticError;

/// Errors that may be returned by the Lido program.
#[derive(Clone, Debug, Eq, Error, FromPrimitive, PartialEq)]
pub enum LidoError {
    /// Address is already initialized
    #[error("AlreadyInUse")]
    AlreadyInUse = 0,
    /// Lido account mismatch the one stored in the Lido program
    #[error("InvalidOwner")]
    InvalidOwner = 1,
    /// Invalid allocated amount
    #[error("InvalidAmount")]
    InvalidAmount = 2,
    /// A required signature is missing
    #[error("SignatureMissing")]
    SignatureMissing = 3,
    /// The reserve account is invalid
    #[error("InvalidReserveAccount")]
    InvalidReserveAccount = 4,
    /// Calculation failed due to division by zero or overflow
    #[error("CalculationFailure")]
    CalculationFailure = 5,
    #[error("WrongStakeState")]
    /// Stake account does not exist or is in an invalid state
    WrongStakeState = 6,
    /// The sum of numerators should be equal to the denominators
    #[error("InvalidFeeAmount")]
    InvalidFeeAmount = 7,
    /// Number of maximum validators reached
    #[error("InvalidFeeAmount")]
    MaximumNumberOfAccountsExceeded = 8,
    /// The size of the account for the Solido state does not match `max_validators`.
    #[error("UnexpectedMaxValidators")]
    UnexpectedMaxValidators = 9,
    /// Wrong manager trying  to alter the state
    #[error("InvalidManager")]
    InvalidManager = 10,
    /// Wrong maintainer trying  to alter the state
    #[error("InvalidMaintainer")]
    InvalidMaintainer = 11,
    /// One of the provided accounts had a mismatch in is_writable or is_signer,
    /// or for a const account, the address does not match the expected address.
    #[error("InvalidAccountInfo")]
    InvalidAccountInfo = 12,
    /// More accounts were provided than the program expects.
    #[error("TooManyAccountKeys")]
    TooManyAccountKeys = 13,
    /// Wrong fee distribution account
    #[error("InvalidFeeDistributionAccount")]
    InvalidFeeDistributionAccount = 14,
    /// Wrong validator credits account
    #[error("InvalidValidatorCreditAccount")]
    InvalidValidatorCreditAccount = 15,
    /// Validator credit account was changed
    #[error("ValidatorCreditChanged")]
    ValidatorCreditChanged = 16,
    /// Fee account should be the same as the Stake pool fee'
    #[error("ValidatorCreditChanged")]
    InvalidFeeAccount = 17,
    /// One of the fee recipients is invalid
    #[error("InvalidFeeRecipient")]
    InvalidFeeRecipient = 18,
    /// There is a stake account with the same key present in the validator
    /// credit list.
    #[error("DuplicatedEntry")]
    DuplicatedEntry = 19,
    /// Validator credit account was not found
    #[error("ValidatorCreditNotFound")]
    ValidatorCreditNotFound = 20,
    /// Validator has unclaimed credit, should mint the tokens before the validator removal
    #[error("ValidatorHasUnclaimedCredit")]
    ValidatorHasUnclaimedCredit = 21,
    /// The reserve account is not rent exempt
    #[error("ReserveIsNotRentExempt")]
    ReserveIsNotRentExempt = 22,
    /// The requested amount for reserve withdrawal exceeds the maximum held in
    /// the reserve account considering rent exemption
    #[error("AmountExceedsReserve")]
    AmountExceedsReserve = 23,
    /// The same maintainer's public key already exists in the structure
    #[error("DuplicatedMaintainer")]
    DuplicatedMaintainer = 24,
    /// A member of the accounts list (maintainers or validators) is not present
    /// in the structure
    #[error("InvalidAccountMember")]
    InvalidAccountMember = 25,
    /// Lido has an invalid size, calculated with the Lido's constant size plus
    /// required to hold variable structures
    #[error("InvalidAccountMember")]
    InvalidLidoSize = 26,
    /// The instance has no validators.
    #[error("EmptySetOfValidators")]
    EmptySetOfValidators = 27,

    /// When staking part of the reserve to a new stake account, the next
    /// program-derived address for the stake account associated with the given
    /// validator, does not match the provided stake account, or the stake account
    /// is not the right account to stake with at this time.
    #[error("InvalidStakeAccount")]
    InvalidStakeAccount = 28,

    /// We expected an SPL token account that holds stSOL,
    /// but this was not an SPL token account,
    /// or its mint did not match.
    #[error("InvalidStSolAccount")]
    InvalidStSolAccount = 29,

    /// The exchange rate has already been updated this epoch.
    #[error("ExchangeRateAlreadyUpToDate")]
    ExchangeRateAlreadyUpToDate = 30,

    /// The exchange rate has not yet been updated this epoch.
    #[error("ExchangeRateNotUpdatedInThisEpoch")]
    ExchangeRateNotUpdatedInThisEpoch = 31,

    /// We observed a decrease in the balance of the validator's stake accounts.
    #[error("ValidatorBalanceDecreased")]
    ValidatorBalanceDecreased = 32,

    /// The provided stake authority does not match the one derived from Lido's state.
    #[error("InvalidStakeAuthority")]
    InvalidStakeAuthority = 33,

    /// The provided rewards withdraw authority does not match the one derived from Lido's state.
    #[error("InvalidRewardsWithdrawAuthority")]
    InvalidRewardsWithdrawAuthority = 34,

    /// The provided Vote Account is invalid or corrupted.
    #[error("InvalidVoteAccount")]
    InvalidVoteAccount = 35,

    /// The provided token owner is different from the given one.
    #[error("InvalidTokenOwner")]
    InvalidTokenOwner = 36,

    /// There is a validator that has more stake than the selected one.
    #[error("ValidatorWithMoreStakeExists")]
    ValidatorWithMoreStakeExists = 37,

    /// The provided mint is invalid.
    #[error("InvalidMint")]
    InvalidMint = 38,

    /// Tried to deposit stake to inactive validator.
    #[error("StakeToInactiveValidator")]
    StakeToInactiveValidator = 39,
}

impl From<ArithmeticError> for LidoError {
    fn from(_: ArithmeticError) -> Self {
        LidoError::CalculationFailure
    }
}

impl From<ArithmeticError> for ProgramError {
    fn from(_: ArithmeticError) -> Self {
        ProgramError::Custom(LidoError::CalculationFailure as u32)
    }
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
