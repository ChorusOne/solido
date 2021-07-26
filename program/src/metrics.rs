// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

//! Data structures for tracking metrics on-chain.
//!
//! In theory, one could look at the transaction history to figure out
//! everything that happened, and compute any metric from there. But in
//! practice, getting the transaction history is not so simple, and extracting
//! anything useful from there is even harder. So what we do instead is embed
//! counters in the on-chain state for the metrics that we are interested in.

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::Serialize;
use solana_program::entrypoint::ProgramResult;

use crate::token::{self, Lamports, StLamports};

#[repr(C)]
#[derive(
    Clone, Debug, Default, BorshDeserialize, BorshSerialize, BorshSchema, Eq, PartialEq, Serialize,
)]
pub struct Metrics {
    /// Fees paid to the treasury, in total since we started tracking, before conversion to stSOL.
    ///
    /// Note: rewards are paid in stSOL, so the treasury did not actually receive
    /// this SOL; it is the SOL that the treasury would have, if it could convert
    /// its fees into SOL immediately after receiving them.
    #[serde(rename = "fee_treasury_total_lamports")]
    pub fee_treasury_sol_total: Lamports,

    /// Fees paid to validators, in total since we started tracking, before conversion to stSOL.
    #[serde(rename = "fee_validation_total_lamports")]
    pub fee_validation_sol_total: Lamports,

    /// Fees paid to the developer, in total since we started tracking, before conversion to stSOL.
    #[serde(rename = "fee_developer_total_lamports")]
    pub fee_developer_sol_total: Lamports,

    /// Total rewards that benefited stSOL holders, in total, since we started tracking.
    #[serde(rename = "st_sol_appreciation_total_lamports")]
    pub st_sol_appreciation_sol_total: Lamports,

    /// Fees paid to the treasury, in total since we started tracking.
    ///
    /// The current value of this stSOL will be different than the value at the
    /// time the fees were paid; [`fee_treasury_sol_total`] tracks the SOL at the
    /// time the fees were paid.
    #[serde(rename = "fee_treasury_total_st_lamports")]
    pub fee_treasury_st_sol_total: StLamports,

    /// Fees paid to validators, in total since we started tracking.
    ///
    /// The current value of this stSOL will be different than the value at the
    /// time the fees were paid; [`fee_validation_sol_total`] tracks the SOL at the
    /// time the fees were paid.
    #[serde(rename = "fee_validation_total_st_lamports")]
    pub fee_validation_st_sol_total: StLamports,

    /// Fees paid to the developer, in total since we started tracking.
    ///
    /// The current value of this stSOL will be different than the value at the
    /// time the fees were paid; [`fee_developer_sol_total`] tracks the SOL at the
    /// time the fees were paid.
    #[serde(rename = "fee_developer_total_st_lamports")]
    pub fee_developer_st_sol_total: StLamports,

    /// Histogram of deposits, including the total amount deposited since we started tracking.
    pub deposit_amount: LamportsHistogram,
    /// Total amount withdrawn since the beginning
    ///
    /// Since the
    pub withdraw_amount: WithdrawMetric,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            fee_treasury_sol_total: Lamports(0),
            fee_validation_sol_total: Lamports(0),
            fee_developer_sol_total: Lamports(0),
            st_sol_appreciation_sol_total: Lamports(0),

            fee_treasury_st_sol_total: StLamports(0),
            fee_validation_st_sol_total: StLamports(0),
            fee_developer_st_sol_total: StLamports(0),

            deposit_amount: LamportsHistogram::new(),
            withdraw_amount: WithdrawMetric::default(),
        }
    }

    pub fn observe_fee_treasury(
        &mut self,
        amount_sol: Lamports,
        amount_st_sol: StLamports,
    ) -> token::Result<()> {
        self.fee_treasury_sol_total = (self.fee_treasury_sol_total + amount_sol)?;
        self.fee_treasury_st_sol_total = (self.fee_treasury_st_sol_total + amount_st_sol)?;

        Ok(())
    }

    pub fn observe_fee_validation(
        &mut self,
        amount_sol: Lamports,
        amount_st_sol: StLamports,
    ) -> token::Result<()> {
        self.fee_validation_sol_total = (self.fee_validation_sol_total + amount_sol)?;
        self.fee_validation_st_sol_total = (self.fee_validation_st_sol_total + amount_st_sol)?;

        Ok(())
    }

    pub fn observe_fee_developer(
        &mut self,
        amount_sol: Lamports,
        amount_st_sol: StLamports,
    ) -> token::Result<()> {
        self.fee_developer_sol_total = (self.fee_developer_sol_total + amount_sol)?;
        self.fee_developer_st_sol_total = (self.fee_developer_st_sol_total + amount_st_sol)?;

        Ok(())
    }

    pub fn observe_reward_st_sol_appreciation(&mut self, amount: Lamports) -> token::Result<()> {
        self.st_sol_appreciation_sol_total = (self.st_sol_appreciation_sol_total + amount)?;

        Ok(())
    }

    pub fn observe_deposit(&mut self, amount: Lamports) -> ProgramResult {
        self.deposit_amount.observe(amount)
    }
    pub fn observe_withdraw(
        &mut self,
        st_sol_amount: StLamports,
        sol_amount: Lamports,
    ) -> token::Result<()> {
        self.withdraw_amount.observe(st_sol_amount, sol_amount)
    }
}

/// A histogram to count SOL values.
///
/// The buckets increment by a factor of 10 each. The smallest bucket is
/// 1e5 Lamports (1e-4 SOL), as the transaction fee per signature 5e3 Lamports,
/// so transferring smaller amounts doesn't make much sense, as you would spend
/// more on the fee than the amount to move. The largest bucket is 1e6 SOL,
/// as the largest validators currently have around 10x that amount of SOL
/// staked, so we are unlikely to see that much deposited in a single transaction.
#[repr(C)]
#[derive(
    Clone, Debug, Default, BorshDeserialize, BorshSerialize, BorshSchema, Eq, PartialEq, Serialize,
)]
pub struct LamportsHistogram {
    /// Histogram buckets.
    ///
    /// `counts[i]` counts how many times a value less than or equal to
    /// `BUCKET_UPPER_BOUNDS[i]` was observed.
    pub counts: [u64; 12],

    /// Sum of all observations.
    #[serde(rename = "total_lamports")]
    pub total: Lamports,
}

impl LamportsHistogram {
    #[rustfmt::skip]
    pub const BUCKET_UPPER_BOUNDS: [Lamports; 12] = [
        Lamports(              100_000),   // 0.000_1 SOL
        Lamports(            1_000_000),     // 0.001 SOL
        Lamports(           10_000_000),      // 0.01 SOL
        Lamports(          100_000_000),       // 0.1 SOL
        Lamports(        1_000_000_000),         // 1 SOL
        Lamports(       10_000_000_000),        // 10 SOL
        Lamports(      100_000_000_000),       // 100 SOL
        Lamports(    1_000_000_000_000),     // 1_000 SOL
        Lamports(   10_000_000_000_000),    // 10_000 SOL
        Lamports(  100_000_000_000_000),   // 100_000 SOL
        Lamports(1_000_000_000_000_000), // 1_000_000 SOL
        Lamports(u64::MAX),
    ];

    pub fn new() -> Self {
        Self {
            counts: [0; 12],
            total: Lamports(0),
        }
    }

    /// Record a new observation.
    pub fn observe(&mut self, amount: Lamports) -> ProgramResult {
        for (count, upper_bound) in self.counts.iter_mut().zip(&Self::BUCKET_UPPER_BOUNDS) {
            if amount <= *upper_bound {
                *count += 1;
            }
        }

        self.total = (self.total + amount)?;

        Ok(())
    }

    pub fn num_observations(&self) -> u64 {
        // Every observation falls in the last bucket, so it contains the total
        // number of observations.
        self.counts[self.counts.len() - 1]
    }
}

/// Track how many times the withdraw function was called, as well as the number
/// of StSOL and SOL that was withdrawn.
#[repr(C)]
#[derive(
    Clone, Debug, Default, BorshDeserialize, BorshSerialize, BorshSchema, Eq, PartialEq, Serialize,
)]
pub struct WithdrawMetric {
    /// Total amount of StSOL withdrawn.
    pub total_st_sol_amount: StLamports,
    /// Total amount of SOL withdrawn, after the conversion.
    pub total_sol_amount: Lamports,
    /// How many times the withdraw function was called.
    pub count: u64,
}

impl WithdrawMetric {
    pub fn observe(
        &mut self,
        st_sol_amount: StLamports,
        sol_amount: Lamports,
    ) -> token::Result<()> {
        self.total_st_sol_amount = (self.total_st_sol_amount + st_sol_amount)?;
        self.total_sol_amount = (self.total_sol_amount + sol_amount)?;
        self.count += 1;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_metrics_observe_fee_treasury() {
        let mut m = Metrics::new();
        m.observe_fee_treasury(Lamports(100), StLamports(100))
            .unwrap();
        m.observe_fee_treasury(Lamports(100), StLamports(80))
            .unwrap();
        assert_eq!(m.fee_treasury_sol_total, Lamports(200));
        assert_eq!(m.fee_treasury_st_sol_total, StLamports(180));
    }

    #[test]
    fn test_metrics_observe_fee_validation() {
        let mut m = Metrics::new();
        m.observe_fee_validation(Lamports(100), StLamports(100))
            .unwrap();
        m.observe_fee_validation(Lamports(100), StLamports(80))
            .unwrap();
        assert_eq!(m.fee_validation_sol_total, Lamports(200));
        assert_eq!(m.fee_validation_st_sol_total, StLamports(180));
    }

    #[test]
    fn test_metrics_observe_fee_developer() {
        let mut m = Metrics::new();
        m.observe_fee_developer(Lamports(100), StLamports(100))
            .unwrap();
        m.observe_fee_developer(Lamports(100), StLamports(80))
            .unwrap();
        assert_eq!(m.fee_developer_sol_total, Lamports(200));
        assert_eq!(m.fee_developer_st_sol_total, StLamports(180));
    }

    #[test]
    fn test_metrics_observe_reward_st_sol_appreciation() {
        let mut m = Metrics::new();
        m.observe_reward_st_sol_appreciation(Lamports(100)).unwrap();
        m.observe_reward_st_sol_appreciation(Lamports(200)).unwrap();
        assert_eq!(m.st_sol_appreciation_sol_total, Lamports(300));
    }

    #[test]
    fn test_metrics_observe_deposit() {
        let mut m = Metrics::new();

        // 0.000_000_100 SOL, falls in bucket 0 (<= 0.000_1 SOL).
        m.observe_deposit(Lamports(100)).unwrap();

        // 1 SOL, falls in bucket 4. (<= 1 SOL)
        m.observe_deposit(Lamports(1_000_000_000)).unwrap();

        // 57 SOL, falls in bucket 6. (<= 100 SOL)
        m.observe_deposit(Lamports(57_000_000_000)).unwrap();

        // 21M SOL, falls in bucket 11. (<= u64::MAX SOL).
        m.observe_deposit(Lamports(21_000_000_000_000_000)).unwrap();

        assert_eq!(m.deposit_amount.counts[0], 1);
        assert_eq!(m.deposit_amount.counts[1], 1);
        assert_eq!(m.deposit_amount.counts[2], 1);
        assert_eq!(m.deposit_amount.counts[3], 1);
        assert_eq!(m.deposit_amount.counts[4], 2);
        assert_eq!(m.deposit_amount.counts[5], 2);
        assert_eq!(m.deposit_amount.counts[6], 3);
        assert_eq!(m.deposit_amount.counts[7], 3);
        assert_eq!(m.deposit_amount.counts[8], 3);
        assert_eq!(m.deposit_amount.counts[9], 3);
        assert_eq!(m.deposit_amount.counts[10], 3);
        assert_eq!(m.deposit_amount.counts[11], 4);

        assert_eq!(m.deposit_amount.num_observations(), 4);
        assert_eq!(m.deposit_amount.total, Lamports(21_000_058_000_000_100));
    }
}
