use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use lido::token::StLamports;
use serde::Serialize;

use crate::token::{self, BLamports, MicroUst};

#[repr(C)]
#[derive(
    Clone, Debug, Default, BorshDeserialize, BorshSerialize, BorshSchema, Eq, PartialEq, Serialize,
)]
pub struct Metrics {
    /// Total swapped amount of StSOL to UST.
    #[serde(rename = "swaped_rewards_total_st_lamports")]
    pub swapped_rewards_st_sol_total: StLamports,

    /// Total amount of UST received through swaps.
    #[serde(rename = "swapped_rewards_ust_total_microust")]
    pub swapped_rewards_ust_total: MicroUst,

    /// Metric for deposits.
    pub deposit_metric: DepositWithdrawMetric,

    /// Metrics for withdrawals.
    pub withdraw_metric: DepositWithdrawMetric,
}

#[repr(C)]
#[derive(
    Clone, Debug, Default, BorshDeserialize, BorshSerialize, BorshSchema, Eq, PartialEq, Serialize,
)]
pub struct DepositWithdrawMetric {
    /// Total amount of StSOL.
    pub st_sol_total: StLamports,

    /// Total amount of bSol.
    pub b_sol_total: BLamports,

    /// Total number of times the metric was called.
    pub count: u64,
}

impl Metrics {
    pub fn new() -> Self {
        let empty_metric = DepositWithdrawMetric {
            st_sol_total: StLamports(0),
            b_sol_total: BLamports(0),
            count: 0,
        };
        Metrics {
            swapped_rewards_st_sol_total: StLamports(0),
            swapped_rewards_ust_total: MicroUst(0),
            deposit_metric: empty_metric.clone(),
            withdraw_metric: empty_metric,
        }
    }

    pub fn observe_token_swap(
        &mut self,
        st_sol_amount: StLamports,
        ust_amount: MicroUst,
    ) -> token::Result<()> {
        self.swapped_rewards_st_sol_total = (self.swapped_rewards_st_sol_total + st_sol_amount)?;
        self.swapped_rewards_ust_total = (self.swapped_rewards_ust_total + ust_amount)?;

        Ok(())
    }

    pub fn observe_deposit(
        &mut self,
        st_sol_amount: StLamports,
        b_sol_amount: BLamports,
    ) -> token::Result<()> {
        self.deposit_metric.st_sol_total = (self.deposit_metric.st_sol_total + st_sol_amount)?;
        self.deposit_metric.b_sol_total = (self.deposit_metric.b_sol_total + b_sol_amount)?;
        self.deposit_metric.count += 1;

        Ok(())
    }

    pub fn observe_withdraw(
        &mut self,
        st_sol_amount: StLamports,
        b_sol_amount: BLamports,
    ) -> token::Result<()> {
        self.withdraw_metric.st_sol_total = (self.withdraw_metric.st_sol_total + st_sol_amount)?;
        self.withdraw_metric.b_sol_total = (self.withdraw_metric.b_sol_total + b_sol_amount)?;
        self.withdraw_metric.count += 1;

        Ok(())
    }
}
