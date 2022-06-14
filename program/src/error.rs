// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

//! Error types

use num_derive::FromPrimitive;
use solana_program::{decode_error::DecodeError, program_error::ProgramError};

use crate::token::ArithmeticError;
use std::fmt::Formatter;

/// Errors that may be returned by the Lido program.
#[derive(Clone, Debug, Eq, FromPrimitive, PartialEq)]
pub enum LidoError {
    /// Address is already initialized
    AlreadyInUse = 0,

    /// Lido account mismatch the one stored in the Lido program
    InvalidOwner = 1,

    /// Invalid allocated amount
    InvalidAmount = 2,

    /// A required signature is missing
    SignatureMissing = 3,

    /// The reserve account is invalid
    InvalidReserveAccount = 4,

    /// Calculation failed due to division by zero or overflow
    CalculationFailure = 5,

    /// Stake account does not exist or is in an invalid state
    WrongStakeState = 6,

    /// The sum of numerators should be equal to the denominators
    InvalidFeeAmount = 7,

    /// Number of maximum validators reached
    MaximumNumberOfAccountsExceeded = 8,

    /// The size of the account for the Solido state does not match `max_validators`.
    UnexpectedMaxValidators = 9,

    /// Wrong manager trying  to alter the state
    InvalidManager = 10,

    /// Wrong maintainer trying  to alter the state
    InvalidMaintainer = 11,

    /// One of the provided accounts had a mismatch in is_writable or is_signer,
    /// or for a const account, the address does not match the expected address.
    InvalidAccountInfo = 12,

    /// More accounts were provided than the program expects.
    TooManyAccountKeys = 13,

    /// Wrong fee distribution account
    InvalidFeeDistributionAccount = 14,

    /// Wrong validator credits account
    InvalidValidatorCreditAccount = 15,

    /// Validator credit account was changed
    ValidatorCreditChanged = 16,

    /// Fee account should be the same as the Stake pool fee'
    InvalidFeeAccount = 17,

    /// One of the fee recipients is invalid
    InvalidFeeRecipient = 18,

    /// There is a stake account with the same key present in the validator
    /// credit list.
    DuplicatedEntry = 19,

    /// Validator credit account was not found
    ValidatorCreditNotFound = 20,

    /// Validator has unclaimed credit, should mint the tokens before the validator removal
    ValidatorHasUnclaimedCredit = 21,

    /// The reserve account is not rent exempt
    ReserveIsNotRentExempt = 22,

    /// The requested amount for reserve withdrawal exceeds the maximum held in
    /// the reserve account considering rent exemption
    AmountExceedsReserve = 23,

    /// The same maintainer's public key already exists in the structure
    DuplicatedMaintainer = 24,

    /// A member of the accounts list (maintainers or validators) is not present
    /// in the structure
    InvalidAccountMember = 25,

    /// Lido has an invalid size, calculated with the Lido's constant size plus
    /// required to hold variable structures
    InvalidLidoSize = 26,

    /// The instance has no validators.
    NoActiveValidators = 27,

    /// When staking part of the reserve to a new stake account, the next
    /// program-derived address for the stake account associated with the given
    /// validator, does not match the provided stake account, or the stake account
    /// is not the right account to stake with at this time.
    InvalidStakeAccount = 28,

    /// We expected an SPL token account that holds stSOL,
    /// but this was not an SPL token account,
    /// or its mint did not match.
    InvalidStSolAccount = 29,

    /// The exchange rate has already been updated this epoch.
    ExchangeRateAlreadyUpToDate = 30,

    /// The exchange rate has not yet been updated this epoch.
    ExchangeRateNotUpdatedInThisEpoch = 31,

    /// We observed a decrease in the balance of the validator's stake accounts.
    ValidatorBalanceDecreased = 32,

    /// The provided stake authority does not match the one derived from Lido's state.
    InvalidStakeAuthority = 33,

    /// The provided rewards withdraw authority does not match the one derived from Lido's state.
    InvalidRewardsWithdrawAuthority = 34,

    /// The provided Vote Account is invalid or corrupted.
    InvalidVoteAccount = 35,

    /// The provided token owner is different from the given one.
    InvalidTokenOwner = 36,

    /// There is a validator that has more stake than the selected one.
    ValidatorWithMoreStakeExists = 37,

    /// The provided mint is invalid.
    InvalidMint = 38,

    /// Tried to deposit stake to inactive validator.
    StakeToInactiveValidator = 39,

    /// Tried to remove a validator when it when it was active or had stake accounts.
    ValidatorIsStillActive = 40,

    /// Tried to remove a validator when it when it was active or had stake accounts.
    ValidatorShouldHaveNoStakeAccounts = 41,

    /// There is a validator that has less stake than the selected one, stake to that one instead.
    ValidatorWithLessStakeExists = 42,

    /// Tried to remove a validator when it when it was active or had stake accounts.
    ValidatorShouldHaveNoUnstakeAccounts = 43,

    /// The validator already has the maximum number of unstake accounts.
    ///
    /// We can't unstake more in this epoch, wait for stake to deactivate, close
    /// the unstake accounts with `WithdrawInactiveStake`, and retry next epoch.
    MaxUnstakeAccountsReached = 44,

    /// The validator's vote account is not owned by the vote program.
    ValidatorVoteAccountHasDifferentOwner = 45,

    /// We expected the StSol account to be owned by the SPL token program.
    InvalidStSolAccountOwner = 46,

    /// Tried to use a deprecated instruction
    InstructionIsDeprecated = 47,

    /// Validation fee is more than 100%
    ValidationFeeOutOfBounds = 48,
}

// Just reuse the generated Debug impl for Display. It shows the variant names.
impl std::fmt::Display for LidoError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
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
