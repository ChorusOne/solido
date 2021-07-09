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
use serde::Serialize;
use std::{
    convert::TryFrom,
    fmt,
    iter::Sum,
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
            Serialize,
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

        impl Sum<$TokenLamports> for Option<$TokenLamports> {
            fn sum<I: Iterator<Item = $TokenLamports>>(iter: I) -> Self {
                let mut sum = $TokenLamports(0);
                for item in iter {
                    sum = (sum + item)?;
                }
                Some(sum)
            }
        }
    };
}

impl_token!(Lamports, "SOL");
impl_token!(StLamports, "stSOL");

/// Parse a numeric string as an amount of SOL.
///
/// Note that this parses the SOL, which can include a decimal point. It does
/// not parse the number of lamports! This makes this function the semi-inverse
/// of `Display` (only `Display` adds the "SOL" suffix, and we do not expect that
/// here).
impl std::str::FromStr for Lamports {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut value = 0_u64;
        let mut is_after_decimal = false;
        let mut exponent = 9_i32;
        let mut had_digit = false;

        // Walk the bytes one by one, we only expect ASCII digits or '.', so bytes
        // suffice. We build up the value as we go, and if we get past the decimal
        // point, we also track how far we are past it.
        for ch in s.as_bytes() {
            match ch {
                b'0'..=b'9' => {
                    value = value * 10 + ((ch - b'0') as u64);
                    if is_after_decimal {
                        exponent -= 1;
                    }
                    had_digit = true;
                },
                b'.' if !is_after_decimal => is_after_decimal = true,
                b'.' => return Err("A SOL value can contain at most one '.' (decimal point)."),
                b'_' => { /* As a courtesy, allow numeric underscores for readability. */ },
                _ => return Err("Invalid SOL value, only digits, '_', and '.' are allowed."),
            }

            if exponent < 0 {
                return Err("A SOL value can contain at most 9 digits after the decimal point.");
            }
        }

        if !had_digit {
            return Err("A SOL value must contain at least one digit.");
        }

        // If the value contained fewer than 9 digits behind the decimal point
        // (or no decimal point at all), scale up the value so it is measured
        // in lamports.
        while exponent > 0 {
            value = value * 10;
            exponent -= 1;
        }

        Ok(Lamports(value))
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_lamports_from_str_roundtrip() {
        let mut x = 0;
        while x < u64::MAX / 17 {
            let orig = Lamports(x);
            let s = format!("{}", orig);
            // Cut off the " SOL" suffix.
            let without_suffix = &s[..s.len() - 4];
            let reconstructed = Lamports::from_str(without_suffix).unwrap();
            assert_eq!(reconstructed, orig);

            x += 1;
            x *= 17;
        }
    }

    #[test]
    fn test_lamports_from_str_examples() {
        assert_eq!(Lamports::from_str("1_000"), Ok(Lamports(1_000_000_000_000)));
        assert_eq!(Lamports::from_str("1"), Ok(Lamports(1_000_000_000)));
        assert_eq!(Lamports::from_str("1."), Ok(Lamports(1_000_000_000)));
        assert_eq!(Lamports::from_str("1.0"), Ok(Lamports(1_000_000_000)));
        assert_eq!(Lamports::from_str("1.02"), Ok(Lamports(1_020_000_000)));
        assert_eq!(Lamports::from_str("1.000_000_001"), Ok(Lamports(1_000_000_001)));
        assert_eq!(Lamports::from_str(".1"), Ok(Lamports(100_000_000)));

        // No digits.
        assert!(Lamports::from_str("").is_err());
        assert!(Lamports::from_str(".").is_err());
        assert!(Lamports::from_str("_").is_err());
        assert!(Lamports::from_str("_._").is_err());

        // Too many digits after decimal point
        assert!(Lamports::from_str("0.000_000_000_1").is_err());

        // More than one decimal point.
        assert!(Lamports::from_str("0.0.0").is_err());

        // Invalid character.
        assert!(Lamports::from_str("lol, sol").is_err());
    }
}
