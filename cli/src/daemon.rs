// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

//! Maintenance daemon that periodically executes maintenance tasks, and serves metrics.
//!
//! The daemon consists of two parts: a main loop, and http server threads. The
//! main loop polls the latest state from the chain through the normal RPC, and
//! executes maintenance tasks if needed. It also publishes a snapshot of its
//! most recently seen Solido state in an `Arc` so the http threads can serve it
//! without blocking the main loop.

use std::io;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rand::{rngs::ThreadRng, Rng};
use tiny_http::{Request, Response, Server};

use crate::config::RunMaintainerOpts;
use crate::error::{AsPrettyError, Error};
use crate::maintenance::{try_perform_maintenance, MaintenanceOutput, SolidoState};
use crate::prometheus::{write_metric, Metric, MetricFamily};
use crate::snapshot::SnapshotError;
use crate::SnapshotClientConfig;

/// Metrics counters that track how many maintenance operations we performed.
#[derive(Clone)]
struct MaintenanceMetrics {
    /// Number of times that we checked if there was maintenance to perform.
    polls: u64,

    /// Number of times that we tried to perform maintenance, but encountered an error.
    errors: u64,

    /// Number of times we performed `StakeDeposit`.
    transactions_stake_deposit: u64,

    /// Number of times we performed `UpdateExchangeRate`.
    transactions_update_exchange_rate: u64,

    /// Number of times we performed `WithdrawInactiveStake`.
    transactions_withdraw_inactive_stake: u64,

    /// Number of times we performed `CollectValidatorFee`
    transactions_collect_validator_fee: u64,

    /// Number of times we performed a `MergeStake`.
    transactions_merge_stake: u64,

    /// Number of times we performed `ClaimValidatorFee`.
    transactions_claim_validator_fee: u64,
    // TODO(#96#issuecomment-859388866): Track how much the daemon spends on transaction fees,
    // so we know how much SOL it costs to operate.
    // spent_lamports_total: u64
    /// Number of times we performed `UnstakeFromInactiveValidator`.
    transactions_unstake_from_inactive_validator: u64,

    /// Number of times we performed `RemoveValidator`.
    transactions_remove_validator: u64,
}

impl MaintenanceMetrics {
    /// Serialize metrics in Prometheus text format.
    pub fn write_prometheus<W: io::Write>(&self, out: &mut W) -> io::Result<()> {
        write_metric(
            out,
            &MetricFamily {
                name: "solido_maintenance_polls_total",
                help:
                    "Number of times we checked if there is maintenance to perform, since launch.",
                type_: "counter",
                metrics: vec![Metric::new(self.polls)],
            },
        )?;
        write_metric(out, &MetricFamily {
            name: "solido_maintenance_errors_total",
            help: "Number of times we encountered an error while trying to perform maintenance, since launch.",
            type_: "counter",
            metrics: vec![Metric::new(self.errors)]
        })?;
        write_metric(
            out,
            &MetricFamily {
                name: "solido_maintenance_transactions_total",
                help: "Number of maintenance transactions executed, since launch.",
                type_: "counter",
                metrics: vec![
                    Metric::new(self.transactions_stake_deposit)
                        .with_label("operation", "StakeDeposit".to_string()),
                    Metric::new(self.transactions_update_exchange_rate)
                        .with_label("operation", "UpdateExchangeRate".to_string()),
                    Metric::new(self.transactions_withdraw_inactive_stake)
                        .with_label("operation", "WithdrawInactiveStake".to_string()),
                    Metric::new(self.transactions_collect_validator_fee)
                        .with_label("operation", "CollectValidatorFee".to_string()),
                    Metric::new(self.transactions_merge_stake)
                        .with_label("operation", "MergeStake".to_string()),
                    Metric::new(self.transactions_claim_validator_fee)
                        .with_label("operation", "ClaimValidatorFee".to_string()),
                    Metric::new(self.transactions_unstake_from_inactive_validator)
                        .with_label("operation", "UnstakeFromInactiveValidator".to_string()),
                    Metric::new(self.transactions_remove_validator)
                        .with_label("operation", "RemoveValidator".to_string()),
                ],
            },
        )?;
        Ok(())
    }

    /// Increment the counter for a maintenance operation.
    pub fn observe_maintenance(&mut self, maintenance_output: &MaintenanceOutput) {
        match *maintenance_output {
            MaintenanceOutput::StakeDeposit { .. } => {
                self.transactions_stake_deposit += 1;
            }
            MaintenanceOutput::UpdateExchangeRate => {
                self.transactions_update_exchange_rate += 1;
            }
            MaintenanceOutput::WithdrawInactiveStake { .. } => {
                self.transactions_withdraw_inactive_stake += 1;
            }
            MaintenanceOutput::CollectValidatorFee { .. } => {
                self.transactions_collect_validator_fee += 1
            }
            MaintenanceOutput::MergeStake { .. } => self.transactions_merge_stake += 1,
            MaintenanceOutput::ClaimValidatorFee { .. } => {
                self.transactions_claim_validator_fee += 1
            }
            MaintenanceOutput::UnstakeFromInactiveValidator { .. } => {
                self.transactions_unstake_from_inactive_validator += 1
            }
            MaintenanceOutput::RemoveValidator { .. } => self.transactions_remove_validator += 1,
        }
    }
}

/// Snapshot of metrics and Solido state.
struct Snapshot {
    /// Metrics about what the daemon has done so far.
    metrics: MaintenanceMetrics,

    /// The current state of on-chain accounts, and the time at which we obtained
    /// that data.
    solido: Option<SolidoState>,
}

enum MaintenanceResult {
    /// We failed to obtain a snapshot of the on-chain state at all, possibly a connectivity problem.
    ErrSnapshot(Error),

    /// We have a state snapshot and there was maintenance to perform, but that failed.
    ErrMaintenance(SolidoState, Error),

    /// We have a state snapshot, and there was no maintenance to perform.
    OkIdle(SolidoState),

    /// We have a state snapshot, and we performed maintenance.
    OkMaintenance(SolidoState, MaintenanceOutput),
}

/// Run a single maintenance iteration.
fn run_maintenance_iteration(
    config: &mut SnapshotClientConfig,
    opts: &RunMaintainerOpts,
) -> MaintenanceResult {
    let result = config.with_snapshot(|mut config| {
        let state = SolidoState::new(&mut config, opts.solido_program_id(), opts.solido_address())?;
        match try_perform_maintenance(&mut config, &state) {
            Ok(None) => Ok(MaintenanceResult::OkIdle(state)),
            Ok(Some(output)) => Ok(MaintenanceResult::OkMaintenance(state, output)),
            Err(SnapshotError::MissingAccount) => Err(SnapshotError::MissingAccount),
            Err(SnapshotError::OtherError(err)) => {
                Ok(MaintenanceResult::ErrMaintenance(state, err))
            }
        }
    });
    match result {
        Err(err) => MaintenanceResult::ErrSnapshot(err),
        Ok(result) => result,
    }
}

/// Mutex that holds the latest snapshot.
///
/// At startup it holds None, after that it will always hold Some Arc.
/// To read the current snapshot, we only have to lock the mutex briefly,
/// so we can clone the arc, and then we can continue to work with that
/// snapshot without any lock. This holds for publishing a new state as well:
/// we can prepare it privately, and we only need to lock the mutex briefly
/// to swap the Arc.
type SnapshotMutex = Mutex<Option<Arc<Snapshot>>>;

struct Daemon<'a, 'b> {
    config: &'a mut SnapshotClientConfig<'b>,

    opts: &'a RunMaintainerOpts,

    /// Random number generator used for exponential backoff with jitter on errors.
    rng: ThreadRng,

    /// The instant after we successfully queried the on-chain state for the last time.
    last_read_success: Instant,

    /// Metrics counters to track status.
    metrics: MaintenanceMetrics,

    /// Mutex where we publish the latest snapshot for use by the webserver.
    snapshot_mutex: Arc<SnapshotMutex>,
}

impl<'a, 'b> Daemon<'a, 'b> {
    pub fn new(config: &'a mut SnapshotClientConfig<'b>, opts: &'a RunMaintainerOpts) -> Self {
        let metrics = MaintenanceMetrics {
            polls: 0,
            errors: 0,
            transactions_stake_deposit: 0,
            transactions_update_exchange_rate: 0,
            transactions_withdraw_inactive_stake: 0,
            transactions_collect_validator_fee: 0,
            transactions_merge_stake: 0,
            transactions_claim_validator_fee: 0,
            transactions_unstake_from_inactive_validator: 0,
            transactions_remove_validator: 0,
        };
        Daemon {
            config,
            opts,
            rng: rand::thread_rng(),
            last_read_success: Instant::now(),
            metrics,
            snapshot_mutex: Arc::new(Mutex::new(None)),
        }
    }

    /// Publish a new snapshot that from now on will be served by the http server.
    fn publish_snapshot(&mut self, solido: Option<SolidoState>) {
        if solido.is_some() {
            self.last_read_success = Instant::now();
        }

        let snapshot = Snapshot {
            metrics: self.metrics.clone(),
            solido,
        };
        self.snapshot_mutex
            .lock()
            .unwrap()
            .replace(Arc::new(snapshot));
    }

    /// Sleep with exponential backoff and jitter.
    fn sleep_after_error(&mut self) {
        // For the sleep time we use exponential backoff with jitter [1]. By taking
        // the time since the last success as the target sleep time, we get
        // exponential backoff. We clamp this to ensure we don't wait indefinitely.
        // 1: https://aws.amazon.com/blogs/architecture/exponential-backoff-and-jitter/
        let time_since_last_success = self.last_read_success.elapsed();
        let min_sleep_time = Duration::from_secs_f32(0.2);
        let max_sleep_time = Duration::from_secs_f32(300.0);
        let target_sleep_time = time_since_last_success.clamp(min_sleep_time, max_sleep_time);
        let sleep_time = self
            .rng
            .gen_range(Duration::from_secs(0)..target_sleep_time);
        println!("Sleeping {:?} after error ...", sleep_time);
        std::thread::sleep(sleep_time);
    }

    /// Sleep either for the configured poll interval, or until it is our maintainer duty.
    ///
    /// TODO(ruuda): Implement the sleeping until maintainer duty part.
    fn sleep_until_next_iteration(&mut self) {
        let sleep_time = Duration::from_secs(*self.opts.max_poll_interval_seconds());
        println!("Sleeping {:?} until next iteration ...", sleep_time);
        std::thread::sleep(sleep_time);
    }

    /// Run maintenance in a loop.
    fn run(mut self) -> ! {
        loop {
            self.metrics.polls += 1;
            match run_maintenance_iteration(self.config, self.opts) {
                MaintenanceResult::ErrSnapshot(err) => {
                    println!("Error while obtaining on-chain state.");
                    err.print_pretty();
                    self.metrics.errors += 1;
                    self.publish_snapshot(None);
                    self.sleep_after_error();
                }
                MaintenanceResult::ErrMaintenance(state, err) => {
                    println!("Error while performing maintenance.");
                    err.print_pretty();
                    self.metrics.errors += 1;
                    self.publish_snapshot(Some(state));
                    // After a failed maintenance transaction, we sleep the regular
                    // poll interval. This ensures that if there is a bug that causes
                    // maintenance transactions to always fail (like [1]), we don't
                    // go in a busy loop submitting failing transactions.
                    // 1: https://github.com/ChorusOne/solido/issues/422
                    self.sleep_until_next_iteration();
                }
                MaintenanceResult::OkIdle(state) => {
                    self.publish_snapshot(Some(state));
                    self.sleep_until_next_iteration();
                }
                MaintenanceResult::OkMaintenance(state, output) => {
                    println!("{}", output);
                    self.metrics.observe_maintenance(&output);
                    self.publish_snapshot(Some(state));
                    // Note, we do not sleep here. If we performed maintenance, we
                    // might not be done yet, so we should immediately check again.
                }
            };
        }
    }
}

fn serve_request(request: Request, snapshot_mutex: &SnapshotMutex) -> Result<(), std::io::Error> {
    // Take the current snapshot. This only holds the lock briefly, and does
    // not prevent other threads from updating the snapshot while this request
    // handler is running.
    let option_snapshot = snapshot_mutex.lock().unwrap().clone();

    // It might be that no snapshot is available yet. This happens when we just
    // started the server, and the main loop has not yet queried the RPC for the
    // latest state.
    let snapshot = match option_snapshot {
        Some(arc_snapshot) => arc_snapshot,
        None => {
            return request.respond(
                Response::from_string(
                    "Service Unavailable\n\nServer is still starting, try again shortly.",
                )
                .with_status_code(503),
            );
        }
    };

    // We don't even look at the request, for now we always serve the metrics.

    let mut out: Vec<u8> = Vec::new();
    let mut is_ok = snapshot.metrics.write_prometheus(&mut out).is_ok();

    if let Some(ref solido) = snapshot.solido {
        is_ok = is_ok && solido.write_prometheus(&mut out).is_ok();
    }

    if is_ok {
        request.respond(Response::from_data(out))
    } else {
        request.respond(Response::from_string("error").with_status_code(500))
    }
}

/// Spawn threads that run the http server.
fn start_http_server(
    opts: &RunMaintainerOpts,
    snapshot_mutex: Arc<SnapshotMutex>,
) -> Vec<JoinHandle<()>> {
    let server = match Server::http(opts.listen().clone()) {
        Ok(server) => Arc::new(server),
        Err(err) => {
            eprintln!(
                "Error: {}\nFailed to start http server on {}. Is the daemon already running?",
                err,
                opts.listen(),
            );
            std::process::exit(1);
        }
    };

    println!("Http server listening on {}", opts.listen());

    // Spawn a number of http handler threads, so we can handle requests in
    // parallel. This server is only used to serve metrics, it can be super basic,
    // but some degree of parallelism is nice in case a client is slow to send
    // its request or something like that.
    (0..num_cpus::get())
        .map(|i| {
            let server_clone = server.clone();
            let snapshot_mutex_clone = snapshot_mutex.clone();
            std::thread::Builder::new()
                .name(format!("http_handler_{}", i))
                .spawn(move || {
                    for request in server_clone.incoming_requests() {
                        // Ignore any errors; if we fail to respond, then there's little
                        // we can do about it here ... the client should just retry.
                        let _ = serve_request(request, &*snapshot_mutex_clone);
                    }
                })
                .expect("Failed to spawn http handler thread.")
        })
        .collect()
}

/// Run the maintenance daemon.
pub fn main(config: &mut SnapshotClientConfig, opts: &RunMaintainerOpts) {
    let daemon = Daemon::new(config, opts);
    let _http_threads = start_http_server(opts, daemon.snapshot_mutex.clone());
    daemon.run();
}
