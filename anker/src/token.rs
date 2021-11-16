use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use lido::impl_token;
use lido::token::{ArithmeticError, Rational};
use serde::Serialize;
use std::{
    convert::TryFrom,
    fmt,
    iter::Sum,
    ops::{Add, Div, Mul, Sub},
};

use lido::token::Result;

impl_token!(BLamports, "bSOL");
