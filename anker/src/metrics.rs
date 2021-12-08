use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use lido::token::StLamports;
use serde::Serialize;

use crate::token::{self, MicroUst};

#[repr(C)]
#[derive(
    Clone, Debug, Default, BorshDeserialize, BorshSerialize, BorshSchema, Eq, PartialEq, Serialize,
)]
pub struct Metrics {
    /// Total swapped amount of StSOL to UST.
    #[serde(rename = "swaped_rewards_total_st_lamports")]
    pub swapped_rewards_st_sol_total: StLamports,

    /// Total swapped amount of UST to StSOL.
    #[serde(rename = "swapped_rewards_ust_total_microust")]
    pub swapped_rewards_ust_total: MicroUst,
}

impl Metrics {
    pub fn new() -> Self {
        Metrics {
            swapped_rewards_st_sol_total: StLamports(0),
            swapped_rewards_ust_total: MicroUst(0),
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
}
