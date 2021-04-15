//! State transition types

use {
    borsh::{BorshDeserialize, BorshSchema, BorshSerialize},
};

/// Initialized program details.
#[repr(C)]
#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
pub struct DepositPool {
    /// Last epoch the `total_stake_lamports` field was updated
    pub amount: u64,
}
