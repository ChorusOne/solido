//! Types to make working with token balances safer.
//!
//! These wrapper types serve a few purposes:
//!
//! * Having distinct types for SOL and stSOL makes it harder to accidentally
//!   perform nonsensical operations on them, such as adding SOL to stSOL.
//! * The wrapper types implement only checked arithmetic, so if you use them,
//!   you can’t forget to check for overflow.
//! * More subtle logic, such as multiplication with a rational, only has to be
//!   implemented once, so the code working with these types can focus on
//!   getting the formulas right, rather than the checked arithmetic bookkeeping.

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use std::{
    convert::TryFrom,
    fmt,
    ops::{Add, Div, Mul, Sub},
};

#[derive(Copy, Clone)]
pub struct Rational {
    pub numerator: u64,
    pub denominator: u64,
}

/// Generate a token type that wraps the minimal unit of the token, it’s
/// “Lamport”. The symbol is for 10<sup>9</sup> of its minimal units and is
/// only used for `Debug` and `Display` printing.
macro_rules! impl_token {
    ($TokenLamports:ident, $symbol:expr) => {
        #[derive(
            Copy,
            Clone,
            Default,
            Eq,
            Ord,
            PartialEq,
            PartialOrd,
            BorshDeserialize,
            BorshSerialize,
            BorshSchema,
        )]
        pub struct $TokenLamports(pub u64);

        impl fmt::Display for $TokenLamports {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(
                    f,
                    "{}.{:0>9} {}",
                    self.0 / 1_000_000_000,
                    self.0 % 1_000_000_000,
                    $symbol
                )
            }
        }

        impl fmt::Debug for $TokenLamports {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                fmt::Display::fmt(self, f)
            }
        }

        impl Mul<Rational> for $TokenLamports {
            type Output = Option<$TokenLamports>;
            fn mul(self, other: Rational) -> Option<$TokenLamports> {
                // This multiplication cannot overflow, because we expand the
                // u64s into u128, and u64::MAX * u64::MAX < u128::MAX.
                let result_u128 = ((self.0 as u128) * (other.numerator as u128))
                    .checked_div(other.denominator as u128)?;
                Some($TokenLamports(u64::try_from(result_u128).ok()?))
            }
        }

        impl Mul<u64> for $TokenLamports {
            type Output = Option<$TokenLamports>;
            fn mul(self, other: u64) -> Option<$TokenLamports> {
                Some($TokenLamports(self.0.checked_mul(other)?))
            }
        }

        impl Div<u64> for $TokenLamports {
            type Output = Option<$TokenLamports>;
            fn div(self, other: u64) -> Option<$TokenLamports> {
                Some($TokenLamports(self.0.checked_div(other)?))
            }
        }

        impl Sub<$TokenLamports> for $TokenLamports {
            type Output = Option<$TokenLamports>;
            fn sub(self, other: $TokenLamports) -> Option<$TokenLamports> {
                Some($TokenLamports(self.0.checked_sub(other.0)?))
            }
        }

        impl Add<$TokenLamports> for $TokenLamports {
            type Output = Option<$TokenLamports>;
            fn add(self, other: $TokenLamports) -> Option<$TokenLamports> {
                Some($TokenLamports(self.0.checked_add(other.0)?))
            }
        }
    };
}

impl_token!(Lamports, "SOL");
impl_token!(StLamports, "stSOL");
impl_token!(StakePoolTokenLamports, "SPT");
