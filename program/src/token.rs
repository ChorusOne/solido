// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

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

#[derive(Copy, Clone, PartialEq, Debug, Serialize)]
pub struct Rational {
    pub numerator: u64,
    pub denominator: u64,
}

impl PartialOrd for Rational {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if self.denominator == 0 || other.denominator == 0 {
            None
        } else {
            let x = self.numerator as u128 * other.denominator as u128;
            let y = other.numerator as u128 * self.denominator as u128;
            Some(x.cmp(&y))
        }
    }
}

impl Div for Rational {
    type Output = f64;

    // We do not return a `Rational` here because `self.numerator *
    // rhs.denominator` or `rhs.numerator * self.denominator`could overflow.
    // Instead we deal with floating point numbers.
    fn div(self, rhs: Self) -> Self::Output {
        (self.numerator as f64 * rhs.denominator as f64)
            / (self.denominator as f64 * rhs.numerator as f64)
    }
}

impl Rational {
    pub fn to_f64(&self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }
}

/// Error returned when a calculation in a token type overflows, underflows, or divides by zero.
#[derive(Debug, Eq, PartialEq)]
pub struct ArithmeticError;

pub type Result<T> = std::result::Result<T, ArithmeticError>;

/// Generate a token type that wraps the minimal unit of the token, it’s
/// “Lamport”. The symbol is for 10<sup>9</sup> of its minimal units and is
/// only used for `Debug` and `Display` printing.
#[macro_export]
macro_rules! impl_token {
    ($TokenLamports:ident, $symbol:expr, decimals = $decimals:expr) => {
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
                    "{}.{} {}",
                    self.0 / 10u64.pow($decimals),
                    &format!("{:0>9}", self.0 % 10u64.pow($decimals))[9 - $decimals..],
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
            type Output = Result<$TokenLamports>;
            fn mul(self, other: Rational) -> Result<$TokenLamports> {
                // This multiplication cannot overflow, because we expand the
                // u64s into u128, and u64::MAX * u64::MAX < u128::MAX.
                let result_u128 = ((self.0 as u128) * (other.numerator as u128))
                    .checked_div(other.denominator as u128)
                    .ok_or(ArithmeticError)?;
                u64::try_from(result_u128)
                    .map($TokenLamports)
                    .map_err(|_| ArithmeticError)
            }
        }

        impl Mul<u64> for $TokenLamports {
            type Output = Result<$TokenLamports>;
            fn mul(self, other: u64) -> Result<$TokenLamports> {
                self.0
                    .checked_mul(other)
                    .map($TokenLamports)
                    .ok_or(ArithmeticError)
            }
        }

        impl Div<u64> for $TokenLamports {
            type Output = Result<$TokenLamports>;
            fn div(self, other: u64) -> Result<$TokenLamports> {
                self.0
                    .checked_div(other)
                    .map($TokenLamports)
                    .ok_or(ArithmeticError)
            }
        }

        impl Sub<$TokenLamports> for $TokenLamports {
            type Output = Result<$TokenLamports>;
            fn sub(self, other: $TokenLamports) -> Result<$TokenLamports> {
                self.0
                    .checked_sub(other.0)
                    .map($TokenLamports)
                    .ok_or(ArithmeticError)
            }
        }

        impl Add<$TokenLamports> for $TokenLamports {
            type Output = Result<$TokenLamports>;
            fn add(self, other: $TokenLamports) -> Result<$TokenLamports> {
                self.0
                    .checked_add(other.0)
                    .map($TokenLamports)
                    .ok_or(ArithmeticError)
            }
        }

        impl Sum<$TokenLamports> for Result<$TokenLamports> {
            fn sum<I: Iterator<Item = $TokenLamports>>(iter: I) -> Self {
                let mut sum = $TokenLamports(0);
                for item in iter {
                    sum = (sum + item)?;
                }
                Ok(sum)
            }
        }
        /// Parse a numeric string as an amount of Lamports, i.e., with 9 digit precision.
        ///
        /// Note that this parses the Lamports amount divided by 10<sup>9</sup>,
        /// which can include a decimal point. It does not parse the number of
        /// Lamports! This makes this function the semi-inverse of `Display`
        /// (only `Display` adds the suffixes, and we do not expect that
        /// here).
        impl std::str::FromStr for $TokenLamports {
            type Err = &'static str;
            fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
                let mut value = 0_u64;
                let mut is_after_decimal = false;
                let mut exponent: i32 = $decimals;
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
                        }
                        b'.' if !is_after_decimal => is_after_decimal = true,
                        b'.' => return Err("Value can contain at most one '.' (decimal point)."),
                        b'_' => { /* As a courtesy, allow numeric underscores for readability. */ }
                        _ => return Err("Invalid value, only digits, '_', and '.' are allowed."),
                    }

                    if exponent < 0 {
                        return Err("Value can contain at most 9 digits after the decimal point.");
                    }
                }

                if !had_digit {
                    return Err("Value must contain at least one digit.");
                }

                // If the value contained fewer than 9 digits behind the decimal point
                // (or no decimal point at all), scale up the value so it is measured
                // in lamports.
                while exponent > 0 {
                    value *= 10;
                    exponent -= 1;
                }

                Ok($TokenLamports(value))
            }
        }
    };
}

impl_token!(Lamports, "SOL", decimals = 9);
impl_token!(StLamports, "stSOL", decimals = 9);

#[cfg(test)]
pub mod test {
    use super::*;
    use std::str::FromStr;

    impl_token!(MicroUst, "UST", decimals = 6);

    #[test]
    fn test_lamports_from_str_roundtrip() {
        let mut x = 0;
        while x < u64::MAX / 17 {
            let lamports_orig = Lamports(x);
            let lamports_string = format!("{}", lamports_orig);
            // Cut off the " SOL" suffix.
            let lamports_without_suffix = &lamports_string[..lamports_string.len() - 4];
            let lamports_reconstructed = Lamports::from_str(lamports_without_suffix).unwrap();
            assert_eq!(lamports_reconstructed, lamports_orig);

            let ust_orig = MicroUst(x);
            let ust_string = format!("{}", ust_orig);
            // Cut off the " UST" suffix.
            let ust_without_suffix = &ust_string[..ust_string.len() - 4];
            let ust_reconstructed = MicroUst::from_str(ust_without_suffix).unwrap();
            assert_eq!(ust_reconstructed, ust_orig);

            x += 1;
            x *= 17;
        }
    }

    #[test]
    fn test_lamports_from_str_handles_more_than_f64() {
        let x = "9007199.254740993";
        let expected = Lamports(9007199_254740993);

        // Parsing as integer from the start should work.
        assert_eq!(Lamports::from_str(x), Ok(expected));

        // Parsing as float and casting to int does not work for this number,
        // because it doesn’t fit the f64 mantissa. If we would parse as f64,
        // we would lose one lamport.
        assert_eq!((f64::from_str(x).unwrap() * 1e9) as u64, expected.0 - 1);
    }

    #[test]
    fn test_lamports_from_str_examples() {
        assert_eq!(Lamports::from_str("1_000"), Ok(Lamports(1_000_000_000_000)));
        assert_eq!(Lamports::from_str("1"), Ok(Lamports(1_000_000_000)));
        assert_eq!(Lamports::from_str("1."), Ok(Lamports(1_000_000_000)));
        assert_eq!(Lamports::from_str("1.0"), Ok(Lamports(1_000_000_000)));
        assert_eq!(Lamports::from_str("1.02"), Ok(Lamports(1_020_000_000)));
        assert_eq!(
            Lamports::from_str("1.000_000_001"),
            Ok(Lamports(1_000_000_001))
        );
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

    #[test]
    fn test_rational_cmp() {
        // Construct x and y such that x < y.
        let x = Rational {
            numerator: 1 << 53,
            denominator: 1,
        };
        let y = Rational {
            numerator: x.numerator + 1,
            denominator: x.denominator,
        };
        assert_eq!(x.partial_cmp(&y), Some(std::cmp::Ordering::Less));
        assert_eq!(y.partial_cmp(&x), Some(std::cmp::Ordering::Greater));
    }

    #[test]
    fn test_equal_cmp() {
        // Construct x and y such that x < y.
        let x = Rational {
            numerator: 1,
            denominator: 1,
        };
        let y = Rational {
            numerator: 1,
            denominator: 1,
        };
        assert_eq!(x.partial_cmp(&y), Some(std::cmp::Ordering::Equal));
        assert_eq!(y.partial_cmp(&x), Some(std::cmp::Ordering::Equal));
    }

    #[test]
    fn test_division_by_zero_cmp() {
        let x = Rational {
            numerator: 1,
            denominator: 0,
        };
        let y = Rational {
            numerator: x.numerator,
            denominator: x.denominator + 1,
        };
        assert_eq!(x.partial_cmp(&y), None);
        assert_eq!(y.partial_cmp(&x), None);
        let y = Rational {
            numerator: x.numerator,
            denominator: x.denominator,
        };
        assert_eq!(x.partial_cmp(&y), None);
        assert_eq!(y.partial_cmp(&x), None);
    }

    #[test]
    fn test_token_format() {
        assert_eq!(format!("{}", Lamports(1)), "0.000000001 SOL");
        assert_eq!(format!("{}", Lamports(1_000_000_002)), "1.000000002 SOL");
        assert_eq!(format!("{}", MicroUst(1)), "0.000001 UST");
        assert_eq!(format!("{}", MicroUst(1_000_000_002)), "1000.000002 UST");
    }

    #[test]
    fn test_division_with_large_number() {
        let x = Rational {
            numerator: 18446744073709551557,
            denominator: 20116405751046403,
        };
        let y = Rational {
            numerator: 69088929115017047,
            denominator: 18446744073709551533,
        };
        let div_result = x / y;
        assert_eq!(div_result, 244838.99999999997); // Checked with WolframAlpha and adjusted to Rust's precision.

        let x = Rational {
            numerator: u64::MAX,
            denominator: u64::MAX,
        };
        let y = Rational {
            numerator: 2,
            denominator: 2,
        };
        let div_result = x / y;
        assert_eq!(div_result, 1.);
    }
}
