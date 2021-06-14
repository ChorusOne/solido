//! Maintenance daemon that periodically executes maintenance tasks, and serves metrics.

use std::io;
use std::sync::Arc;
use std::time::Duration;
use std::thread::JoinHandle;

use clap::Clap;
use rand::Rng;
use solana_sdk::pubkey::Pubkey;
use tiny_http::{Server, Response, Request};

use crate::maintenance::{PerformMaintenanceOpts, MaintenanceOutput, perform_maintenance};
use crate::prometheus::{MetricFamily, Metric, write_metric};
use crate::{Config, Error};

#[derive(Clap, Clone, Debug)]
pub struct RunMaintainerOpts {
    /// Address of the Solido program.
    #[clap(long)]
    pub solido_program_id: Pubkey,

    /// Account that stores the data for this Solido instance.
    #[clap(long)]
    pub solido_address: Pubkey,

    /// Stake pool program id
    #[clap(long)]
    pub stake_pool_program_id: Pubkey,

    /// Listen address and port for the http server that serves a /metrics endpoint.
    #[clap(long, default_value = "0.0.0.0:8923")]
    pub listen: String,

    /// Maximum time to wait after there was no maintenance to perform, before checking again.
    ///
    /// The expected wait time is half the max poll interval. A max poll interval
    /// of a few minutes should be plenty fast for a production deployment, but
    /// for testing you can reduce this value to make the daemon more responsive,
    /// to eliminate some waiting time.
    #[clap(long, default_value = "120")]
    pub max_poll_interval_seconds: u64,
}

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

/// Run the maintenance loop
pub fn run_main_loop(
    config: &Config,
    opts: &RunMaintainerOpts,
) {
    let mut metrics = MaintenanceMetrics {
        polls: 0,
        errors: 0,
        transactions_stake_deposit: 0,
        transactions_deposit_active_stake_to_pool: 0,
    };
    let mut rng = rand::thread_rng();

    // The perform-maintenance options are a subset of the run-maintainer options.
    let maintenance_opts = PerformMaintenanceOpts {
        solido_address: opts.solido_address,
        solido_program_id: opts.solido_program_id,
        stake_pool_program_id: opts.stake_pool_program_id,
    };

    loop {
        let mut do_wait = false;
        match perform_maintenance(config, &maintenance_opts) {
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
            let max_poll_interval = Duration::from_secs(opts.max_poll_interval_seconds);
            let sleep_time = rng.gen_range(Duration::from_secs(0)..max_poll_interval);
            std::thread::sleep(sleep_time);
        }
    }
}

pub fn serve_request(request: Request) {
    println!("received request! method: {:?}, url: {:?}, headers: {:?}",
             request.method(),
             request.url(),
             request.headers()
    );

    let response = Response::from_string("hello world");
    request.respond(response);
}

/// Spawn threads that run the http server.
pub fn start_http_server(opts: &RunMaintainerOpts) -> Vec<JoinHandle<()>> {
    let server = Arc::new(Server::http(opts.listen.clone()).unwrap());
    println!("Http server listening on {}", opts.listen);

    // Spawn a number of http handler threads, so we can handle requests in
    // parallel. This server is only used to serve metrics, it can be super basic,
    // but some degree of parallelism is nice in case a client is slow to send
    // its request or something like that.
    (0..8)
        .map(|i| {
            let server_clone = server.clone();
            std::thread::Builder::new().name(format!("http_handler_{}", i)).spawn(move || {
                for request in server_clone.incoming_requests() {
                    serve_request(request);
                }
            }).expect("Failed to spawn http handler thread.")
        })
        .collect()
}

/// Run the maintenance daemon.
pub fn main(
    config: &Config,
    opts: &RunMaintainerOpts,
) {
    let http_threads = start_http_server(&opts);

    run_main_loop(config, opts);

    // We never get here, the main loop should run indefinitely until the program
    // is killed, and while the main loop runs, the http server also serves.
    for thread in http_threads {
        thread.join().unwrap();
    }
}
