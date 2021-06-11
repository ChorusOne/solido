//! Maintenance daemon that periodically executes maintenance tasks, and serves metrics.

use crate::{Config, Error};
use crate::maintenance::PerformMaintenanceOpts;
use std::time::Duration;

/// Time to wait after there was no maintenance to perform, before checking again.
const POLL_INTERVAL: Duration = Duration::from_secs(120);

/// Metrics counters that track how many maintenance operations we performed.
struct MaintenanceMetrics {
    /// Number of times that we checked if there was maintenance to perform.
    polls_total: u64,

    /// Number of times we performed `DepositStake`.
    calls_stake_deposit_total: u64,

    /// Number of times we performed `DepositActiveStakeToPool`.
    calls_deposit_active_stake_to_pool_total: u64,

    // TODO(#96#issuecomment-859388866): Track how much the daemon spends on transaction fees,
    // so we know how much SOL it costs to operate.
    // spent_lamports_total: u64
}

/// Run the maintenance daemon.
pub fn main(
    config: &Config,
    opts: PerformMaintenanceOpts,
) {

}