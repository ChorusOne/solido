use std::fmt::Formatter;

use num_derive::FromPrimitive;
use solana_program::{decode_error::DecodeError, program_error::ProgramError};

/// Errors that may be returned by the Anker program.
///
/// Note: the integer representations of these errors start counting at 4000,
/// to avoid them overlapping with the Solido errors. When a Solana program fails,
/// all we get is the error code, and if we use the same "namespace" of small
/// integers, we can't tell from the error alone which program it was that failed.
/// This matters in the CLI client where we print all possible interpretations of
/// the error code.
#[derive(Clone, Debug, Eq, FromPrimitive, PartialEq)]
pub enum AnkerError {
    /// We failed to deserialize an SPL token account.
    InvalidTokenAccount = 4000,

    /// We expected the SPL token account to be owned by the SPL token program.
    InvalidTokenAccountOwner = 4001,

    /// The mint of a provided SPL token account does not match the expected mint.
    InvalidTokenMint = 4002,

    /// The provided reserve is invalid.
    InvalidReserveAccount = 4003,

    /// The provided Solido state is different from the stored one.
    InvalidSolidoInstance = 4004,

    /// The one of the provided accounts does not match the expected derived address.
    InvalidDerivedAccount = 4005,

    /// An account is not owned by the expected owner.
    InvalidOwner = 4006,

    /// Wrong SPL Token Swap instance or program.
    WrongSplTokenSwap = 4007,

    /// Wrong parameters for the SPL Token Swap instruction.
    WrongSplTokenSwapParameters = 4008,

    /// The provided rewards destination is different from what is stored in the instance.
    InvalidRewardsDestination = 4009,

    /// The amount of rewards to be claimed are zero.
    ZeroRewardsToClaim = 4010,

    /// Arguments/Accounts for SendRewards are wrong.
    InvalidSendRewardsParameters = 4011,

    /// After swapping, we are left with less stSOL than we intended.
    TokenSwapAmountInvalid = 4012,

    /// The most recent price sample is too recent, we can’t call `FetchPoolPrice` yet.
    FetchPoolPriceTooEarly = 4013,

    /// We failed to compute the price of stSOL in UST.
    PoolPriceUndefined = 4014,

    /// `FetchPoolPrice` has not been called recently, we must call it before selling the rewards.
    FetchPoolPriceNotCalledRecently = 4015,

    /// Value of `sell_rewards_min_out_bps` is greater than 100% (1_000_000).
    InvalidSellRewardsMinBps = 4016,
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
