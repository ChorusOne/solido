//! Maintenance daemon that periodically executes maintenance tasks, and serves metrics.

use crate::prometheus::{MetricFamily, Metric, write_metric};
use crate::{Config, Error};
use crate::maintenance::{PerformMaintenanceOpts, MaintenanceOutput, perform_maintenance};
use std::time::Duration;
use rand::Rng;
use std::io;

/// Maximum time to wait after there was no maintenance to perform, before checking again.
///
/// The expected wait time is half the max poll interval.
const MAX_POLL_INTERVAL: Duration = Duration::from_secs(120);

/// Metrics counters that track how many maintenance operations we performed.
struct MaintenanceMetrics {
    /// Number of times that we checked if there was maintenance to perform.
    polls: u64,

    /// Number of times that we tried to perform maintenance, but encountered an error.
    errors: u64,

    /// Number of times we performed `StakeDeposit`.
    transactions_stake_deposit: u64,

    /// Number of times we performed `DepositActiveStakeToPool`.
    transactions_deposit_active_stake_to_pool: u64,

    // TODO(#96#issuecomment-859388866): Track how much the daemon spends on transaction fees,
    // so we know how much SOL it costs to operate.
    // spent_lamports_total: u64
}

impl MaintenanceMetrics {
    /// Serialize metrics in Prometheus text format.
    pub fn write_prometheus<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        write_metric(out, &MetricFamily {
            name: "solido_maintenance_polls_total",
            help: "Number of times we checked if there is maintenance to perform, since launch.",
            type_: "counter",
            metrics: vec![Metric::simple(self.polls)]
        })?;
        write_metric(out, &MetricFamily {
            name: "solido_maintenance_errors_total",
            help: "Number of times we encountered an error while trying to perform maintenance, since launch.",
            type_: "counter",
            metrics: vec![Metric::simple(self.polls)]
        })?;
        write_metric(out, &MetricFamily {
            name: "solido_maintenance_transactions_total",
            help: "Number of maintenance transactions executed, since launch.",
            type_: "counter",
            metrics: vec![
                Metric::singleton("operation", "StakeDeposit", self.transactions_stake_deposit),
                Metric::singleton("operation", "DepositActiveStakeToPool", self.transactions_deposit_active_stake_to_pool),
            ],
        })?;
        Ok(())
    }
}

/// Run the maintenance daemon.
pub fn main(
    config: &Config,
    opts: PerformMaintenanceOpts,
) {
    let mut metrics = MaintenanceMetrics {
        polls: 0,
        errors: 0,
        transactions_stake_deposit: 0,
        transactions_deposit_active_stake_to_pool: 0,
    };
    let mut rng = rand::thread_rng();

    loop {
        let mut do_wait = false;
        match perform_maintenance(config, &opts) {
            Err(err) => {
                println!("Error in maintenance: {:?}", err);
                metrics.errors += 1;

                // If the error was caused by a connectivity problem, we shouldn't
                // hammer the RPC again straight away. Even better would be to do
                // exponential backoff with jitter, but let's not go there right now.
                do_wait = true;
            }
            Ok(None) => {
                // Nothing to be done, try again later.
                do_wait = true
            },
            Ok(Some(something_done)) => {
                println!("{}", something_done);
                match something_done {
                    MaintenanceOutput::StakeDeposit {..} => {
                        metrics.transactions_stake_deposit += 1
                    },
                    MaintenanceOutput::DepositActiveStateToPool {..} => {
                        metrics.transactions_deposit_active_stake_to_pool += 1
                    },
                }
            }
        }
        metrics.polls += 1;

        if do_wait {
            // Sleep a random time, to avoid a thundering herd problem, in case
            // multiple maintainer bots happened to run in sync. They would all
            // try to create the same transaction, and only one would pass.
            let sleep_time = rng.gen_range(Duration::from_secs(0)..MAX_POLL_INTERVAL);
            std::thread::sleep(sleep_time);
        }
    }
}