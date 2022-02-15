use std::{
    borrow::Cow,
    ops::Range,
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use clap::Clap;
use lido::token::Rational;
use rand::{rngs::ThreadRng, Rng};
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    clock::{Epoch, Slot},
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::Keypair,
};
use solido_cli_common::{
    error::{AsPrettyError, Error},
    prometheus::{write_metric, Metric, MetricFamily},
    snapshot::{Config, OutputMode, SnapshotClient, SnapshotClientConfig},
};
use std::str::FromStr;
use tiny_http::{Header, Request, Response, ResponseBox, Server};
use url::{form_urlencoded, Url};

const SOLIDO_ID: &str = "solido";

#[derive(Clap, Debug)]
pub struct Opts {
    /// Solido's instance address
    #[clap(long)]
    solido_address: Pubkey,

    /// URL of cluster to connect to (e.g., https://api.devnet.solana.com for solana devnet)
    #[clap(long, default_value = "http://127.0.0.1:8899")]
    cluster: String,

    /// Poll interval in seconds.
    #[clap(long, default_value = "300")]
    poll_interval_seconds: u32,

    /// Location of the SQLite DB file.
    #[clap(long, default_value = "listener.sqlite3")]
    db_path: String,

    /// Listen address and port for the http server.
    #[clap(long, default_value = "0.0.0.0:8929")]
    listen: String,
}

#[derive(Debug)]
pub struct ExchangeRate {
    /// Id of the data point.
    id: i32,
    /// Time when the data point was logged.
    timestamp: chrono::DateTime<chrono::Utc>,
    /// Slot when the data point was logged.
    slot: Slot,
    /// Epoch when the data point was logged.
    epoch: Epoch,
    /// Pool identifier, e.g. for Solido would be "solido".
    pool: String,
    /// Price of token A.
    price_lamports_numerator: u64,
    /// Price of token B.
    price_lamports_denominator: u64,
}

pub fn create_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS exchange_rate (
                id          INTEGER PRIMARY KEY,
                --- timestamp is stored in ISO-8601 format.
                timestamp                   TEXT,
                slot                        INTEGER NOT NULL,
                epoch                       INTEGER NOT NULL,
                pool                        TEXT NOT NULL,
                price_lamports_numerator    INTEGER NOT NULL,
                price_lamports_denominator  INTEGER NOT NULL,
                CHECK (price_lamports_denominator>0)
            )",
        [],
    )?;
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct IntervalPrices {
    begin_datetime: chrono::DateTime<chrono::Utc>,
    end_datetime: chrono::DateTime<chrono::Utc>,
    begin_epoch: Epoch,
    end_epoch: Epoch,
    begin_token_price_sol: Rational,
    end_token_price_sol: Rational,
}

impl IntervalPrices {
    pub fn duration_wall_time(&self) -> chrono::Duration {
        self.end_datetime - self.begin_datetime
    }
    pub fn duration_epochs(&self) -> u64 {
        self.end_epoch - self.begin_epoch
    }
    pub fn growth_factor(&self) -> f64 {
        self.end_token_price_sol / self.begin_token_price_sol
    }
    pub fn annual_growth_factor(&self) -> f64 {
        let year = chrono::Duration::days(365);
        self.growth_factor()
            .powf(year.num_seconds() as f64 / self.duration_wall_time().num_seconds() as f64)
    }
    pub fn annual_percentage_rate(&self) -> f64 {
        self.annual_growth_factor().mul_add(100.0, -100.0)
    }
}

impl std::fmt::Display for IntervalPrices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let duration = self.end_datetime - self.begin_datetime;
        writeln!(
            f,
            "Interval price:\n  From: {} (epoch {})\n  To  : {} (epoch {})\n  Average {} days APY: {}",
            self.begin_datetime,
            self.begin_epoch,
            self.end_datetime,
            self.end_epoch,
            duration.num_days(),
            self.annual_percentage_rate()
        )
    }
}

fn parse_utc_iso8601(date_str: &str) -> chrono::ParseResult<chrono::DateTime<chrono::Utc>> {
    Ok(chrono::DateTime::from_utc(
        chrono::DateTime::parse_from_rfc3339(date_str)?.naive_utc(),
        chrono::Utc,
    ))
}

pub fn get_interval_price_for_period(
    tx: rusqlite::Transaction,
    from_time: chrono::DateTime<chrono::Utc>,
    to_time: chrono::DateTime<chrono::Utc>,
    pool: String,
) -> rusqlite::Result<Option<IntervalPrices>> {
    let row_map = |row: &Row| {
        let timestamp: String = row.get(1)?;
        let timestamp =
            chrono::DateTime::from_str(&timestamp).expect("Invalid timestamp format stored in DB.");
        Ok(ExchangeRate {
            id: row.get(0)?,
            timestamp,
            slot: row.get(2)?,
            epoch: row.get(3)?,
            pool: row.get(4)?,
            price_lamports_numerator: row.get(5)?,
            price_lamports_denominator: row.get(6)?,
        })
    };

    let (first, last) = {
        // Get first logged minimal logged data based on timestamp that is greater than `from_time`.
        // TODO(516): Do not limit the query below, but select the first data point
        // that is offset by 200 data points from the selected epoch
        let stmt_first = &mut tx.prepare(
            "WITH prices_epoch AS (
                SELECT *
                FROM exchange_rate
                WHERE epoch = (SELECT MIN(epoch) from exchange_rate where pool = :pool AND timestamp > :t)
              )
              SELECT
                *
              FROM
                prices_epoch
              ORDER BY
                timestamp ASC
            ",
        )?;
        // Get first logged maximal logged data based on timestamp that is smaller than `to_time`.
        let stmt_last =
            &mut tx.prepare("WITH prices_epoch AS (
                SELECT *
                FROM exchange_rate
                WHERE epoch = (SELECT MAX(epoch) from exchange_rate where pool = :pool AND timestamp < :t)
              )
              SELECT
                *
              FROM
                prices_epoch
              ORDER BY
                timestamp ASC
            ")?;
        let mut row_iter = stmt_first.query_map([pool.clone(), from_time.to_rfc3339()], row_map)?;
        let first = row_iter.next();

        let mut row_iter = stmt_last.query_map([pool, to_time.to_rfc3339()], row_map)?;
        let last = row_iter.next();

        (first, last)
    };

    match (first, last) {
        (Some(first), Some(last)) => {
            let first = first?;
            let last = last?;
            // Not enough data, need at least two data points.
            if first.id == last.id {
                Ok(None)
            } else {
                let interval_prices = IntervalPrices {
                    begin_datetime: first.timestamp,
                    end_datetime: last.timestamp,
                    begin_epoch: first.epoch,
                    end_epoch: last.epoch,
                    begin_token_price_sol: Rational {
                        numerator: first.price_lamports_numerator,
                        denominator: first.price_lamports_denominator,
                    },
                    end_token_price_sol: Rational {
                        numerator: last.price_lamports_numerator,
                        denominator: last.price_lamports_denominator,
                    },
                };
                Ok(Some(interval_prices))
            }
        }
        _ => Ok(None),
    }
}

pub fn insert_price(conn: &Connection, exchange_rate: &ExchangeRate) -> rusqlite::Result<()> {
    conn.execute("INSERT INTO exchange_rate (timestamp, slot, epoch, pool, price_lamports_numerator, price_lamports_denominator) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", 
    params![exchange_rate.timestamp.to_rfc3339(), exchange_rate.slot, exchange_rate.epoch, exchange_rate.pool,
        exchange_rate.price_lamports_numerator, exchange_rate.price_lamports_denominator])?;
    Ok(())
}

type MetricsMutex = Mutex<Arc<Metrics>>;
struct Daemon<'a, 'b> {
    config: &'a mut SnapshotClientConfig<'b>,
    opts: &'a Opts,

    /// Mutex where we publish the latest metrics for use by the webserver.
    metrics_snapshot: Arc<MetricsMutex>,

    /// Metrics counters to track status.
    metrics: Metrics,

    /// Random number generator used for exponential backoff with jitter on errors.
    rng: ThreadRng,

    /// The instant after we successfully queried the on-chain state for the last time.
    last_read_success: Instant,

    /// Database connection
    db_connection: &'a Connection,
}

impl<'a, 'b> Daemon<'a, 'b> {
    pub fn new(
        config: &'a mut SnapshotClientConfig<'b>,
        opts: &'a Opts,
        db_connection: &'a Connection,
    ) -> Self {
        let empty_metrics = Metrics {
            polls: 0,
            errors: 0,
            solido_average_30d_interval_price: None,
        };
        Daemon {
            config,
            opts,
            metrics_snapshot: Arc::new(Mutex::new(Arc::new(empty_metrics.clone()))),
            metrics: empty_metrics,
            rng: rand::thread_rng(),
            last_read_success: Instant::now(),
            db_connection,
        }
    }

    fn run(mut self) -> ! {
        loop {
            self.metrics.polls += 1;
            let sleep_time = match get_and_save_exchange_rate(
                self.config,
                self.opts,
                self.db_connection,
                "solido".to_owned(),
            ) {
                ListenerResult::ErrSnapshot(err) => {
                    println!("Error while obtaining on-chain state.");
                    err.print_pretty();
                    self.metrics.errors += 1;
                    self.get_sleep_time_after_error()
                }
                ListenerResult::OkListener(exchange_rate, interval_prices_option) => {
                    println!(
                        "Got exchange rate: {}/{}: {} at slot {} and epoch {}.",
                        exchange_rate.price_lamports_numerator,
                        exchange_rate.price_lamports_denominator,
                        exchange_rate.price_lamports_numerator as f32
                            / exchange_rate.price_lamports_denominator as f32,
                        exchange_rate.slot,
                        exchange_rate.epoch,
                    );

                    match interval_prices_option {
                        None => println!(
                            "No interval price could be produced, awaiting more data points"
                        ),
                        Some(interval_prices) => {
                            println!("30d APY: {}", interval_prices);
                            self.metrics.solido_average_30d_interval_price = Some(interval_prices);
                        }
                    }
                    self.get_sleep_time()
                }
                ListenerResult::ErrListener(err) => {
                    println!("Error in listener.");
                    err.print_pretty();
                    self.get_sleep_time_after_error()
                }
            };
            // Update metrics snapshot.
            *self.metrics_snapshot.lock().unwrap() = Arc::new(self.metrics.clone());
            std::thread::sleep(sleep_time);
        }
    }

    fn get_sleep_time_after_error(&mut self) -> Duration {
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
        sleep_time
    }

    pub fn get_sleep_time(&self) -> Duration {
        // Sleep until is time to get the next exchange rate.
        let sleep_time = std::time::Duration::from_secs(self.opts.poll_interval_seconds as u64);
        println!(
            "Sleeping for {:?} after getting the Solido exchange rate",
            sleep_time
        );
        sleep_time
    }
}

#[derive(Clone)]
struct Metrics {
    /// Number of times that we checked the price.
    polls: u64,

    /// Number of times that we tried to get the exchange rate, but encountered an error.
    errors: u64,

    /// Solido's maximum price interval.
    solido_average_30d_interval_price: Option<IntervalPrices>,
}

impl Metrics {
    pub fn write_prometheus<W: std::io::Write>(&self, out: &mut W) -> std::io::Result<()> {
        write_metric(
            out,
            &MetricFamily {
                name: "solido_pricedb_polls_total",
                help: "Number of times we polled the exchange rate, since launch.",
                type_: "counter",
                metrics: vec![Metric::new(self.polls)],
            },
        )?;
        write_metric(out, &MetricFamily {
            name: "solido_pricedb_errors_total",
            help: "Number of times we encountered an error while trying to get the exchange rate, since launch.",
            type_: "counter",
            metrics: vec![Metric::new(self.errors)]
        })?;
        if let Some(interval_price) = &self.solido_average_30d_interval_price {
            write_metric(
                out,
                &MetricFamily {
                    name: "solido_pricedb_30d_average_apy",
                    help: "Average 30d APY",
                    type_: "gauge",
                    metrics: vec![Metric::new(interval_price.annual_percentage_rate())],
                },
            )?;
        }

        Ok(())
    }
}

#[derive(Serialize, PartialEq, Debug)]
struct ResponseError {
    error: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct ResponseInterval {
    interval_prices: IntervalPrices,
    annual_percentage_rate: f64,
}

type DateFromTo = Range<chrono::ParseResult<chrono::DateTime<chrono::Utc>>>;

/// Returns a `DateFromTo` with the Result from parsing the iso8601 from
/// `input`. `input` is the url query in bytes.
fn get_date_params(parsed_inputs: Vec<(Cow<str>, Cow<str>)>) -> Result<DateFromTo, ResponseError> {
    let (first_k, first_v) = parsed_inputs.get(0).ok_or(ResponseError {
        error: "First parameter \"from\" or \"to\" not provided.".to_owned(),
    })?;
    let (second_k, second_v) = parsed_inputs.get(1).ok_or(ResponseError {
        error: "Second parameter \"from\" or \"to\" not provided.".to_owned(),
    })?;
    match (
        (first_k.as_ref(), first_v.as_ref()),
        (second_k.as_ref(), second_v.as_ref()),
    ) {
        (("from", from), ("to", to)) | (("to", to), ("from", from)) => {
            Ok(parse_utc_iso8601(from)..parse_utc_iso8601(to))
        }
        _ => Err(ResponseError {
            error: "Wrong parameters provided, query parameters should be \"from\" and \"to\"."
                .to_owned(),
        }),
    }
}

/// Returns a Request response with an error.
/// With the status code set to 400.
fn get_error_response(err_res: ResponseError) -> ResponseBox {
    let content_type = Header::from_bytes(
        &b"Content-Type"[..],
        &b"application/json; charset=UTF-8"[..],
    )
    .expect("Static header value, does not fail at runtime.");
    Response::from_data(serde_json::to_vec(&err_res).expect("Serialization shouldn't fail"))
        .with_status_code(400)
        .with_header(content_type)
        .boxed()
}

/// Get an interval price, consume it and returns a `ResponseBox` with the
/// provided interval price and computed annual percentage rate.
fn get_success_response(interval_prices: IntervalPrices) -> ResponseBox {
    let response_interval = ResponseInterval {
        annual_percentage_rate: interval_prices.annual_percentage_rate(),
        interval_prices,
    };
    let content_type = Header::from_bytes(
        &b"Content-Type"[..],
        &b"application/json; charset=UTF-8"[..],
    )
    .expect("Static header value, does not fail at runtime.");
    Response::from_data(
        serde_json::to_vec(&response_interval).expect("Serialization shouldn't fail"),
    )
    .with_header(content_type)
    .boxed()
}

/// Gets a response that will be sent to a requesting client for a price
/// interval. Returns None if no method is passed.
fn get_interval_price_request(
    db_connection: &Connection,
    request_url: &str,
) -> Option<ResponseBox> {
    let parsed_url = match Url::parse(request_url) {
        Ok(parsed_url) => parsed_url,
        Err(err) => {
            return Some(get_error_response(ResponseError {
                error: err.to_string(),
            }))
        }
    };
    let method_name = parsed_url.path_segments()?.last()?;
    if method_name != "interval_price" {
        return Some(get_error_response(ResponseError {
                        error: format!("Method not supported: {}, use \"/interval_price?from=<from_date_iso8601>&to=<to_date_iso8601>\"", method_name)
                    }));
    }
    let parsed_request_url =
        form_urlencoded::parse(parsed_url.query()?.as_bytes()).collect::<Vec<_>>();

    let dates = match get_date_params(parsed_request_url) {
        Ok(res) => Some(res),
        Err(err) => return Some(get_error_response(err)),
    }?;

    match (dates.start, dates.end) {
        (Ok(from), Ok(to)) => {
            let interval_prices = get_interval_price_for_period(
                db_connection
                    .unchecked_transaction()
                    .expect("Failed to create sqlite transaction."),
                from,
                to,
                SOLIDO_ID.to_owned(),
            );
            let interval_prices = interval_prices;
            match interval_prices {
                // Error while getting the interval prices.
                Err(err) => Some(get_error_response(ResponseError {
                    error: err.to_string(),
                })),
                Ok(interval_prices_opt) => {
                    if let Some(interval_prices) = interval_prices_opt {
                        // Got interval prices.
                        Some(get_success_response(interval_prices))
                    } else {
                        // No interval price could be calculated, probably because of few data points.
                        Some(get_error_response(ResponseError {
                            error: "Not enough data points for calculating the price interval."
                                .to_owned(),
                        }))
                    }
                }
            }
        }
        errors => {
            // Some errors happened while parsing date.
            let (from_res, to_res) = errors;
            let mut error_str = "".to_owned();
            if let Err(from_err) = from_res {
                error_str = error_str + "parsing \"from\" date: " + &from_err.to_string();
            }
            if let Err(to_err) = to_res {
                error_str = error_str + "parsing \"to\" date " + &to_err.to_string();
            }
            Some(get_error_response(ResponseError { error: error_str }))
        }
    }
}

fn serve_request(
    db_connection: &Connection,
    request: Request,
    metrics_mutex: &MetricsMutex,
) -> Result<(), std::io::Error> {
    if let Some(res) = get_interval_price_request(db_connection, request.url()) {
        return request.respond(res);
    };

    // Take the current snapshot. This only holds the lock briefly, and does
    // not prevent other threads from updating the snapshot while this request
    // handler is running.
    let metrics = metrics_mutex.lock().unwrap().clone();

    // We don't even look at the request, for now we always serve the metrics.

    let mut out: Vec<u8> = Vec::new();
    metrics.write_prometheus(&mut out).expect(
        "We must handle the error because of io::Write, but writing to a Vec does not fail.",
    );

    // text/plain with version=0.0.4 is what Prometheus expects as the content type,
    // see also https://prometheus.io/docs/instrumenting/exposition_formats/.
    // We add the charset so you can view the metrics in a browser too when it
    // contains non-ascii bytes.
    let content_type = Header::from_bytes(
        &b"Content-Type"[..],
        &b"text/plain; version=0.0.4; charset=UTF-8"[..],
    )
    .expect("Static header value, does not fail at runtime.");
    request.respond(Response::from_data(out).with_header(content_type))
}

/// Spawn threads that run the http server.
fn start_http_server(opts: &Opts, metrics_mutex: Arc<MetricsMutex>) -> Vec<JoinHandle<()>> {
    let server = match Server::http(opts.listen.clone()) {
        Ok(server) => Arc::new(server),
        Err(err) => {
            eprintln!(
                "Error: {}\nFailed to start http server on {}. Is the daemon already running?",
                err, &opts.listen,
            );
            std::process::exit(1);
        }
    };

    println!("Http server listening on {}", &opts.listen);

    // Spawn a number of http handler threads, so we can handle requests in
    // parallel.
    (0..num_cpus::get())
        .map(|i| {
            // Create one db connection per thread.
            let conn = Connection::open(&opts.db_path).expect("Failed to open sqlite connection.");
            let server_clone = server.clone();
            let snapshot_mutex_clone = metrics_mutex.clone();
            std::thread::Builder::new()
                .name(format!("http_handler_{}", i))
                .spawn(move || {
                    for request in server_clone.incoming_requests() {
                        // Ignore any errors; if we fail to respond, then there's little
                        // we can do about it here ... the client should just retry.
                        let _ = serve_request(&conn, request, &*snapshot_mutex_clone);
                    }
                })
                .expect("Failed to spawn http handler thread.")
        })
        .collect()
}

enum ListenerResult {
    /// We failed to obtain a snapshot of the on-chain state at all, possibly a connectivity problem.
    ErrSnapshot(Error),

    /// We have a snapshot, and we got the price.
    OkListener(ExchangeRate, Option<IntervalPrices>),

    /// We have a snapshot, but failed in-between, e.g. when inserting in database.
    ErrListener(Error),
}

/// Save the exchange rate and get a response for the 30d interval price.
fn get_and_save_exchange_rate(
    config: &mut SnapshotClientConfig,
    opts: &Opts,
    db_connection: &Connection,
    pool: String,
) -> ListenerResult {
    let result = config.with_snapshot(|config| {
        let solido = config.client.get_solido(&opts.solido_address)?;
        let clock = config.client.get_clock()?;
        Ok(ExchangeRate {
            id: 0,
            timestamp: chrono::Utc::now(),
            slot: clock.slot,
            epoch: clock.epoch,
            pool: pool.clone(),
            price_lamports_numerator: solido.exchange_rate.sol_balance.0,
            price_lamports_denominator: solido.exchange_rate.st_sol_supply.0,
        })
    });

    match result {
        Err(err) => ListenerResult::ErrSnapshot(err),
        Ok(exchange_rate) => {
            match insert_price_and_query_30d_price_interval(db_connection, &exchange_rate) {
                Ok(interval_prices) => ListenerResult::OkListener(exchange_rate, interval_prices),
                Err(error) => ListenerResult::ErrListener(Box::new(error)),
            }
        }
    }
}

/// Insert an `exchange_rate` into the database and query the 30 days APY from
/// the current date.
fn insert_price_and_query_30d_price_interval(
    db_connection: &Connection,
    exchange_rate: &ExchangeRate,
) -> Result<Option<IntervalPrices>, rusqlite::Error> {
    insert_price(db_connection, exchange_rate)?;
    let tx = db_connection.unchecked_transaction()?;
    let now = chrono::Utc::now();
    let now_minus_30d = now - chrono::Duration::days(30);
    let interval_prices =
        get_interval_price_for_period(tx, now_minus_30d, now, SOLIDO_ID.to_owned())?;
    Ok(interval_prices)
}

fn main() {
    let opts = Opts::parse();
    solana_logger::setup_with_default("solana=info");
    let rpc_client =
        RpcClient::new_with_commitment(opts.cluster.clone(), CommitmentConfig::confirmed());
    let snapshot_client = SnapshotClient::new(rpc_client);

    // Our config has a signer, which for this program we will not use, since we
    // only observe information from the Solana blockchain.
    let signer = Keypair::new();
    let mut config = Config {
        client: snapshot_client,
        signer: &signer,
        output_mode: OutputMode::Text,
    };

    let conn = Connection::open(&opts.db_path).expect("Failed to open sqlite connection.");
    create_db(&conn).expect("Failed to create database.");

    let daemon = Daemon::new(&mut config, &opts, &conn);
    let _http_threads = start_http_server(&opts, daemon.metrics_snapshot.clone());
    daemon.run()
}

#[cfg(test)]
mod test {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_get_average_apy() {
        let conn = Connection::open_in_memory().expect("Failed to open sqlite connection.");
        create_db(&conn).unwrap();
        let exchange_rate = ExchangeRate {
            id: 0,
            timestamp: chrono::Utc.ymd(2020, 8, 8).and_hms(0, 0, 0),
            slot: 1,
            epoch: 1,
            pool: SOLIDO_ID.to_owned(),
            price_lamports_numerator: 1,
            price_lamports_denominator: 1,
        };
        insert_price(&conn, &exchange_rate).unwrap();
        let exchange_rate = ExchangeRate {
            id: 0,
            timestamp: chrono::Utc.ymd(2021, 1, 8).and_hms(0, 0, 0),
            slot: 2,
            epoch: 2,
            pool: SOLIDO_ID.to_owned(),
            price_lamports_numerator: 1394458971361025,
            price_lamports_denominator: 1367327673971744,
        };
        insert_price(&conn, &exchange_rate).unwrap();
        let apy = get_interval_price_for_period(
            conn.unchecked_transaction().unwrap(),
            chrono::Utc.ymd(2020, 7, 7).and_hms(0, 0, 0),
            chrono::Utc.ymd(2021, 7, 8).and_hms(0, 0, 0),
            SOLIDO_ID.to_owned(),
        )
        .expect("Failed when getting APY for period");
        assert_eq!(apy.unwrap().annual_percentage_rate(), 4.7989255185326485);
    }

    // When computing the APY, we have to call `growth_factor` which divides two
    // Rational numbers. Previously when dividing two rationals our implementation
    // returned another Rational. In other words, for dividing `a/b` by `c/d`, we
    // did `a*d/b*c`. In this case, `a*d` or `b*c` could overflow, we now return an
    // `f64` instead of a Rational and avoid multiplying two large numbers that
    // could overflow.
    #[test]
    fn test_rationals_do_not_overflow() {
        let conn = Connection::open_in_memory().expect("Failed to open sqlite connection.");
        create_db(&conn).unwrap();
        let exchange_rate = ExchangeRate {
            id: 0,
            timestamp: chrono::Utc.ymd(2022, 01, 28).and_hms(11, 58, 39),
            slot: 108837851,
            epoch: 270,
            pool: SOLIDO_ID.to_owned(),
            price_lamports_numerator: 1936245653069130,
            price_lamports_denominator: 1893971837707973,
        };
        insert_price(&conn, &exchange_rate).unwrap();

        let exchange_rate = ExchangeRate {
            id: 0,
            timestamp: chrono::Utc.ymd(2022, 02, 28).and_hms(11, 58, 39),
            slot: 118837851,
            epoch: 275,
            pool: SOLIDO_ID.to_owned(),
            price_lamports_numerator: 1936245653069130,
            price_lamports_denominator: 1892971837707973,
        };
        insert_price(&conn, &exchange_rate).unwrap();

        let apy = get_interval_price_for_period(
            conn.unchecked_transaction().unwrap(),
            chrono::Utc.ymd(2020, 7, 7).and_hms(0, 0, 0),
            chrono::Utc.ymd(2022, 7, 8).and_hms(0, 0, 0),
            SOLIDO_ID.to_owned(),
        )
        .expect("Failed when getting APY for period");
        let growth_factor = apy.unwrap().growth_factor();
        assert_eq!(growth_factor, 1.0005282698770684); //  Checked on WA, precision difference in the last digit.
    }

    #[test]
    fn test_get_date_from_url_parameters() {
        let query_params = form_urlencoded::Serializer::new(String::new())
            .append_pair("from", "2022-02-04T11:40:02.683960+00:00")
            .append_pair("to", "2022-02-07T14:22:08.826526+00:00")
            .finish();

        let dates =
            get_date_params(form_urlencoded::parse(query_params.as_bytes()).collect::<Vec<_>>());
        assert_eq!(
            dates,
            Ok(parse_utc_iso8601("2022-02-04T11:40:02.683960+00:00")
                ..parse_utc_iso8601("2022-02-07T14:22:08.826526+00:00")),
        );
    }

    #[test]
    fn test_get_interval_from_url() {
        // check for correct and specific prices.
        fn check_correct_url(conn: &Connection, url: &str) {
            let response: ResponseBox = get_interval_price_request(conn, url).unwrap();
            let mut response_string = String::new();

            // Assert status code is ok.
            assert_eq!(response.status_code(), 200);

            response
                .into_reader()
                .read_to_string(&mut response_string)
                .unwrap();

            let expected_response = ResponseInterval {
                interval_prices: IntervalPrices {
                    begin_datetime: chrono::Utc.ymd(2020, 8, 8).and_hms(0, 0, 0),
                    end_datetime: chrono::Utc.ymd(2021, 1, 8).and_hms(0, 0, 0),
                    begin_epoch: 1,
                    end_epoch: 2,
                    begin_token_price_sol: Rational {
                        numerator: 1,
                        denominator: 1,
                    },
                    end_token_price_sol: Rational {
                        numerator: 1394458971361025,
                        denominator: 1367327673971744,
                    },
                },
                annual_percentage_rate: 4.7989255185326485,
            };

            let response_result: ResponseInterval =
                serde_json::de::from_str(&response_string).unwrap();
            assert_eq!(response_result, expected_response);
        }

        let conn = Connection::open_in_memory().expect("Failed to open sqlite connection.");
        create_db(&conn).unwrap();
        let exchange_rate = ExchangeRate {
            id: 0,
            timestamp: chrono::Utc.ymd(2020, 8, 8).and_hms(0, 0, 0),
            slot: 1,
            epoch: 1,
            pool: SOLIDO_ID.to_owned(),
            price_lamports_numerator: 1,
            price_lamports_denominator: 1,
        };
        insert_price(&conn, &exchange_rate).unwrap();
        let exchange_rate = ExchangeRate {
            id: 0,
            timestamp: chrono::Utc.ymd(2021, 1, 8).and_hms(0, 0, 0),
            slot: 2,
            epoch: 2,
            pool: SOLIDO_ID.to_owned(),
            price_lamports_numerator: 1394458971361025,
            price_lamports_denominator: 1367327673971744,
        };
        insert_price(&conn, &exchange_rate).unwrap();

        // Check some formats return ok.
        check_correct_url(&conn, "http://solana.lido.fi/api/apy/interval_price?from=2020-07-07T00:00:00.683960%2B00:00&to=2021-07-08T14:22:08.826526%2B00:00");
        check_correct_url(&conn, "http://solana.lido.fi/api/apy/interval_price?from=2020-07-07T00:00:00.683960Z&to=2021-07-08T14:22:08.826526Z");
        check_correct_url(&conn, "http://solana.lido.fi/api/apy/interval_price?from=2020-07-07T00%3A00:00.683960%2B00:00&to=2021-07-08T14:22%3A08.826526%2B00:00");
    }
}
