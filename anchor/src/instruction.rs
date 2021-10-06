use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use lido::{
    impl_token,
    token::{ArithmeticError, Rational, Result, StLamports},
};
use serde::Serialize;
use std::{
    convert::TryFrom,
    fmt,
    iter::Sum,
    ops::{Add, Div, Mul, Sub},
};

impl_token!(BLamports, "bSOL");

#[repr(C)]
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum AnchorInstruction {
    Initialize {},

    /// Deposit a given amount of StSOL, gets bSOL in return.
    ///
    /// This can be called by anybody.
    Deposit {
        #[allow(dead_code)] // but it's not
        amount: StLamports,
    },

    /// Withdraw a given amount of stSOL.
    ///
    /// Caller provides some `amount` of StLamports that are to be burned in
    /// order to withdraw bSOL.
    Withdraw {
        #[allow(dead_code)] // but it's not
        amount: StLamports,
    },
}
