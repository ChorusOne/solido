// SPDX-FileCopyrightText: 2022 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

use std::{
    borrow::Cow,
    ops::Range,
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
    vec,
};

use chrono::TimeZone;
use clap::Parser;
use lido::token::Rational;
use rand::{rngs::ThreadRng, Rng};
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    clock::{Clock, Epoch, Slot, SLOT_MS},
    commitment_config::CommitmentConfig,
    epoch_schedule::EpochSchedule,
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
use url::Url;

// Name put in the solido table `pool`.
const SOLIDO_ID: &str = "solido";

// Offset from the first Epoch's slot to use as data point.
const QUERY_SLOT_OFFSET: u64 = 1000;

#[derive(Parser, Debug)]
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

    /// Disable fetching data from the chain.
    ///
    /// By default, the daemon will do two things:
    /// 1. Fetch price data from the chain and save it to the database.
    /// 2. Serve the API to query APY, which reads from the database.
    /// Read-only mode disables 1 while keeping 2 enabled, which ensures that
    /// the application does not write to the database.
    #[clap(long, takes_value = false)]
    read_only: bool,
}

#[derive(Debug, Clone)]
pub struct ExchangeRate {
    /// Id of the data point.
    #[allow(dead_code)]
    pub id: i32,
    /// Time when the data point was logged.
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Slot when the data point was logged.
    pub slot: Slot,
    /// Epoch when the data point was logged.
    pub epoch: Epoch,
    /// Pool identifier, e.g. for Solido would be "solido".
    pub pool: String,
    /// Price of token A.
    pub price_lamports_numerator: u64,
    /// Price of token B.
    pub price_lamports_denominator: u64,
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
            );
            CREATE INDEX IF NOT EXISTS ix_exchange_rate_timestamp ON exchange_rate (timestamp);
            CREATE INDEX IF NOT EXISTS ix_exchange_rate_slot ON exchange_rate (slot);
            ",
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

    pub fn annual_percentage_yield(&self) -> f64 {
        self.annual_growth_factor().mul_add(100.0, -100.0)
    }

    pub fn has_one_data_point() {}
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
            self.annual_percentage_yield()
        )
    }
}

fn parse_utc_iso8601(date_str: &str) -> chrono::ParseResult<chrono::DateTime<chrono::Utc>> {
    date_str.parse()
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
    // This is the constructor for mainnet.
    let epoch_schedule = EpochSchedule::without_warmup();

    let (first, last) = {
        // Get minimum epoch in which timestamp is greater than `from_time`.
        let mut first_epoch_stmt = tx.prepare(
            "SELECT MIN(epoch) from exchange_rate where pool = :pool and timestamp > :t",
        )?;
        let epoch = match first_epoch_stmt
            .query_row([pool.clone(), from_time.to_rfc3339()], |row| {
                row.get::<usize, u64>(0)
            }) {
            Ok(epoch) => epoch,
            Err(_) => return Ok(None),
        };
        let minimum_slot = epoch_schedule.get_first_slot_in_epoch(epoch) + QUERY_SLOT_OFFSET;

        // Get the first row from `epoch` in which the slot is greater than `minimum_slot`.
        let mut exchange_rate_stmt = tx.prepare(
            "SELECT * from exchange_rate WHERE pool = :pool AND epoch = :epoch AND slot >= :slot_min LIMIT 1",
        )?;
        let first_exchange_rate = exchange_rate_stmt
            .query_map(
                [pool.clone(), epoch.to_string(), minimum_slot.to_string()],
                row_map,
            )?
            .next();

        // Get maximum epoch in which timestamp is smaller than `to_time`.
        let mut last_epoch_stmt = tx.prepare(
            "SELECT MAX(epoch) from exchange_rate where pool = :pool and timestamp < :t",
        )?;
        let epoch = match last_epoch_stmt.query_row([pool.clone(), to_time.to_rfc3339()], |row| {
            row.get::<usize, u64>(0)
        }) {
            Ok(epoch) => epoch,
            Err(_) => return Ok(None),
        };
        let minimum_slot = epoch_schedule.get_first_slot_in_epoch(epoch) + QUERY_SLOT_OFFSET;

        // Get the first row from `epoch` in which the slot is greater than `minimum_slot`.
        let mut exchange_rate_stmt = tx.prepare(
            "SELECT * from exchange_rate WHERE pool = :pool AND epoch = :epoch AND slot >= :slot_min LIMIT 1",
        )?;
        let last_exchange_rate = exchange_rate_stmt
            .query_map([pool, epoch.to_string(), minimum_slot.to_string()], row_map)?
            .next();

        (first_exchange_rate, last_exchange_rate)
    };

    match (first, last) {
        (Some(first), Some(last)) => {
            let first = first?;
            let last = last?;
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
        _ => Ok(None),
    }
}

pub fn insert_price(conn: &Connection, exchange_rate: &ExchangeRate) -> rusqlite::Result<()> {
    conn.execute("INSERT INTO exchange_rate (timestamp, slot, epoch, pool, price_lamports_numerator, price_lamports_denominator) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", 
    params![exchange_rate.timestamp.to_rfc3339(), exchange_rate.slot, exchange_rate.epoch, exchange_rate.pool,
        exchange_rate.price_lamports_numerator, exchange_rate.price_lamports_denominator])?;
    Ok(())
}

pub struct Snapshot {
    /// Metrics about the daemon.
    pub metrics: Metrics,

    /// Clock instance, so we know what time it is.
    pub clock: Option<Clock>,
}

pub type MetricsMutex = Mutex<Arc<Snapshot>>;

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
        let current_clock = config
            .with_snapshot(|config| config.client.get_clock())
            .ok();

        Daemon {
            config,
            opts,
            metrics_snapshot: Arc::new(Mutex::new(Arc::new(Snapshot {
                metrics: empty_metrics.clone(),
                clock: current_clock,
            }))),
            metrics: empty_metrics,
            rng: rand::thread_rng(),
            last_read_success: Instant::now(),
            db_connection,
        }
    }

    fn run(mut self) -> ! {
        loop {
            self.metrics.polls += 1;
            let (sleep_time, current_clock) = match get_and_save_exchange_rate(
                self.config,
                self.opts,
                self.db_connection,
                "solido".to_owned(),
            ) {
                ListenerResult::ErrSnapshot(err) => {
                    println!("Error while obtaining on-chain state.");
                    err.print_pretty();
                    self.metrics.errors += 1;
                    (self.get_sleep_time_after_error(), None)
                }
                ListenerResult::OkListener(
                    exchange_rate,
                    interval_prices_option,
                    current_clock,
                ) => {
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
                    (self.get_sleep_time(), Some(current_clock))
                }
                ListenerResult::ErrListener(err, current_clock) => {
                    println!("Error in listener.");
                    err.print_pretty();
                    (self.get_sleep_time_after_error(), Some(current_clock))
                }
            };
            // Update metrics snapshot.
            *self.metrics_snapshot.lock().unwrap() = Arc::new(Snapshot {
                metrics: self.metrics.clone(),
                clock: current_clock,
            });
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
pub struct Metrics {
    /// Number of times that we checked the price.
    pub polls: u64,

    /// Number of times that we tried to get the exchange rate, but encountered an error.
    pub errors: u64,

    /// Solido's maximum price interval.
    pub solido_average_30d_interval_price: Option<IntervalPrices>,
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
                    metrics: vec![Metric::new(interval_price.annual_percentage_yield())],
                },
            )?;
        }

        Ok(())
    }
}

#[derive(Serialize, PartialEq, Debug)]
enum ResponseError {
    BadRequest(&'static str),
    NotFound(&'static str),
    InternalServerError,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct ResponseInterval {
    interval_prices: IntervalPrices,
    annual_percentage_yield: f64,
}

#[derive(Debug, PartialEq)]
enum DateRequestType {
    Fixed,
    MovingTarget,
}

#[derive(Debug, PartialEq)]
struct DateRequest {
    date_range: Range<chrono::DateTime<chrono::Utc>>,
    request_type: DateRequestType,
}

fn get_date_params<'a, I: IntoIterator<Item = (Cow<'a, str>, Cow<'a, str>)>>(
    query_params: I,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<DateRequest, ResponseError> {
    let mut begin_vec: Vec<chrono::DateTime<chrono::Utc>> = vec![];
    let mut end_vec: Vec<chrono::DateTime<chrono::Utc>> = vec![];

    for (k, v) in query_params {
        match k.as_ref() {
            "begin" => {
                let t = parse_utc_iso8601(&v).map_err(|_| {
                    ResponseError::BadRequest(
                        "Invalid ISO 8601 timestamp in 'begin' query parameter. \
                    Expected e.g. '2022-02-15T23:59:59+00:00'. Note that query parameters must be url-encoded",
                    )
                })?;

                begin_vec.push(t);
            }
            "end" => {
                let t = parse_utc_iso8601(&v).map_err(|_| {
                    ResponseError::BadRequest(
                        "Invalid ISO 8601 timestamp in 'end' query parameter. \
                    Expected e.g. '2022-02-15T23:59:59+00:00'. Note that query parameters must be url-encoded.",
                    )
                })?;

                end_vec.push(t);
            }
            "days" => {
                let days = v.parse::<i64>().map_err(|_| {
                    ResponseError::BadRequest(
                        "Invalid number of days in 'days' query parameter. \
                    Expected e.g. '30'.",
                    )
                })?;
                let end = now;
                let begin = end
                    .checked_sub_signed(chrono::Duration::days(days))
                    .ok_or(ResponseError::BadRequest("Date range too large"))?;

                begin_vec.push(begin);
                end_vec.push(end);
            }
            "since_launch" => {
                let begin = chrono::Utc.ymd(2021, 9, 1).and_hms(00, 00, 00); // Solido Launch Date
                let end = now;

                begin_vec.push(begin);
                end_vec.push(end);
            }
            _ => continue,
        }
    }

    let begin = match begin_vec.len() {
        0 => {
            return Err(ResponseError::BadRequest(
                "Missing query parameter: 'begin', 'days' or 'since_launch'",
            ))
        }
        1 => begin_vec[0],
        _ => {
            return Err(ResponseError::BadRequest(
                "Exactly one of 'begin' or 'days' or 'since_launch' must be specified.",
            ))
        }
    };

    let end = match end_vec.len() {
        0 => {
            return Err(ResponseError::BadRequest(
                "Missing query parameter: 'end', 'days' or 'since_launch'",
            ))
        }
        1 => end_vec[0],
        _ => {
            return Err(ResponseError::BadRequest(
                "Exactly one of 'end' or 'days' or 'since_launch' must be specified.",
            ))
        }
    };
    // Requests for `days` and `since_launch` have `end == now`, we include
    // requests when the end is greater than `now` as well as a `MovingTarget`
    // request.
    let request_type = if end >= now {
        DateRequestType::MovingTarget
    } else {
        DateRequestType::Fixed
    };

    Ok(DateRequest {
        date_range: begin..end,
        request_type,
    })
}

/// Returns a Request response with an error depending on `err_res` type.
fn get_error_response(err_res: ResponseError) -> ResponseBox {
    let content_type = Header::from_bytes(&b"Content-Type"[..], &b"text/plain; charset=UTF-8"[..])
        .expect("Static header value, does not fail at runtime.");
    match err_res {
        ResponseError::BadRequest(msg) => Response::from_string(msg)
            .with_status_code(400)
            .with_header(content_type)
            .boxed(),
        ResponseError::NotFound(msg) => Response::from_string(msg)
            .with_status_code(404)
            .with_header(content_type)
            .boxed(),
        ResponseError::InternalServerError => Response::from_string("internal server error")
            .with_status_code(500)
            .with_header(content_type)
            .boxed(),
    }
}

/// Get an interval price, consume it and returns a `ResponseBox` with the
/// provided interval price and computed annual percentage rate.
fn get_success_response(interval_prices: IntervalPrices) -> ResponseBox {
    let response_interval = ResponseInterval {
        annual_percentage_yield: interval_prices.annual_percentage_yield(),
        interval_prices,
    };
    let content_type = Header::from_bytes(
        &b"Content-Type"[..],
        &b"application/json; charset=UTF-8"[..],
    )
    .expect("Static header value, does not fail at runtime.");
    let allow_access_control_origin =
        Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..])
            .expect("Static header value, does not fail at runtime.");

    Response::from_data(
        serde_json::to_vec(&response_interval).expect("Serialization shouldn't fail"),
    )
    .with_header(content_type)
    .with_header(allow_access_control_origin)
    .boxed()
}

/// Get a header with `Expires` key.
/// If the end date for the query is far in the past we set it to expire in 1
/// week. However, if the end date is in the same epoch, we set it to expire in
/// the approximate time the next epoch begins.
fn get_expires_header(
    now: chrono::DateTime<chrono::Utc>,
    interval_prices: &IntervalPrices,
    current_clock: Option<Clock>,
    date_request_type: &DateRequestType,
) -> Header {
    let expires_date = match date_request_type {
        // From the past, set expiration to 1 week from now.
        DateRequestType::Fixed => now + chrono::Duration::weeks(1),
        DateRequestType::MovingTarget => {
            // We don't have information about the current clock, default to 5
            // minutes cache.
            match current_clock {
                None => now + chrono::Duration::minutes(5),
                Some(clock) => {
                    let epoch_sched = EpochSchedule::without_warmup();
                    let first_slot_next_epoch =
                        epoch_sched.get_first_slot_in_epoch(clock.epoch + 1);
                    let slots_diff = first_slot_next_epoch - clock.slot;
                    if interval_prices.duration_epochs() > 0 {
                        // We can estimate the epoch duration.
                        let epoch_duration = interval_prices.duration_wall_time()
                            / interval_prices.duration_epochs() as i32;
                        let slot_duration = epoch_duration
                            / epoch_sched.get_slots_in_epoch(interval_prices.end_epoch) as i32;
                        // Cache until we estimate that 80% of the current epoch will be complete,
                        // so that we don’t cache for too long if the epoch suddenly ends faster than expected.
                        now + slot_duration * (slots_diff * 8 / 10) as i32
                    } else {
                        // We can't estimate the epoch duration, use Solana's
                        // `SLOT_MS`. Empirically, this is lower than than what
                        // we observe in reality, so we'd cache for less time
                        // than ideal.
                        let ms_diff = slots_diff * SLOT_MS;
                        now + chrono::Duration::milliseconds(ms_diff as i64)
                    }
                }
            }
        }
    };

    // Set minimum expiration time to 5 minutes.
    let expires_date = expires_date.max(now + chrono::Duration::minutes(5));
    Header::from_bytes(&b"Expires"[..], &expires_date.to_rfc2822()[..])
        .expect("Static header value, does not fail at runtime.")
}

/// Gets a response that will be sent to a requesting client for an APY
/// query.
fn get_interval_price_request(
    db_connection: &Connection,
    range_date: &DateRequest,
    now: chrono::DateTime<chrono::Utc>,
    current_clock: Option<Clock>,
) -> ResponseBox {
    let interval_prices = get_interval_price_for_period(
        db_connection
            .unchecked_transaction()
            .expect("Failed to create sqlite transaction."),
        range_date.date_range.start,
        range_date.date_range.end,
        SOLIDO_ID.to_owned(),
    );
    match interval_prices {
        // Error while getting the interval prices.
        Err(err) => {
            eprintln!("Internal Error when getting interval prices: {}", err);
            get_error_response(ResponseError::InternalServerError)
        }
        Ok(interval_prices_opt) => {
            if let Some(interval_prices) = interval_prices_opt {
                // Got interval prices.
                let expires_header = get_expires_header(
                    now,
                    &interval_prices,
                    current_clock,
                    &range_date.request_type,
                );
                let mut response = get_success_response(interval_prices);

                response.add_header(expires_header);
                response
            } else {
                // No interval price could be calculated, probably because of few data points.
                get_error_response(ResponseError::NotFound(
                    "No data points for calculating the price interval.",
                ))
            }
        }
    }
}

enum Endpoint {
    Metrics,
    IntervalPriceRequest(DateRequest),
}

fn parse_url(
    request_url: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Endpoint, ResponseError> {
    // `Url::parse` needs the base URL, which is not given by the
    // `request.url()` from `tiny_url`. We input some dummy data which it's
    // never used.
    let base = Url::parse("http://unused.invalid/").expect("Hard-coded value is valid.");
    let parse_result = Url::options().base_url(Some(&base)).parse(request_url);
    let parsed_url = parse_result.map_err(|_| ResponseError::BadRequest("Failed to parse url."))?;

    let last_second_last = parsed_url
        .path_segments()
        .map(|it| it.rev())
        .map(|mut p| (p.next(), p.next()));

    match last_second_last {
        Some((Some("apy"), _)) => {
            get_date_params(parsed_url.query_pairs(), now).map(Endpoint::IntervalPriceRequest)
        }
        Some((Some("metrics"), None)) => Ok(Endpoint::Metrics),
        _ => Err(ResponseError::NotFound("Unknown route.")),
    }
}

pub fn serve_request(
    db_connection: &Connection,
    request: Request,
    metrics_mutex: &MetricsMutex,
) -> Result<(), std::io::Error> {
    let now = chrono::Utc::now();

    let response = match parse_url(request.url(), now) {
        Ok(Endpoint::Metrics) => {
            // Take the current snapshot. This only holds the lock briefly, and does
            // not prevent other threads from updating the snapshot while this request
            // handler is running.
            let snapshot = metrics_mutex.lock().unwrap().clone();

            // We don't even look at the request, for now we always serve the metrics.

            let mut out: Vec<u8> = Vec::new();
            snapshot.metrics.write_prometheus(&mut out).expect(
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
            // request.respond(Response::from_data(out).with_header(content_type));
            Response::from_data(out).with_header(content_type).boxed()
        }
        Ok(Endpoint::IntervalPriceRequest(range_date)) => {
            let current_clock = metrics_mutex.lock().unwrap().clock.clone();
            get_interval_price_request(db_connection, &range_date, now, current_clock)
        }
        Err(err) => get_error_response(err),
    };
    request.respond(response)
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
    OkListener(ExchangeRate, Option<IntervalPrices>, Clock),

    /// We have a snapshot, but failed in-between, e.g. when inserting in database.
    ErrListener(Error, Clock),
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
        Ok((
            ExchangeRate {
                id: 0,
                timestamp: chrono::Utc::now(),
                slot: clock.slot,
                epoch: clock.epoch,
                pool: pool.clone(),
                price_lamports_numerator: solido.exchange_rate.sol_balance.0,
                price_lamports_denominator: solido.exchange_rate.st_sol_supply.0,
            },
            clock,
        ))
    });

    match result {
        Err(err) => ListenerResult::ErrSnapshot(err),
        Ok((exchange_rate, clock)) => {
            match insert_price_and_query_30d_price_interval(db_connection, &exchange_rate) {
                Ok(interval_prices) => {
                    ListenerResult::OkListener(exchange_rate, interval_prices, clock)
                }
                Err(error) => ListenerResult::ErrListener(Box::new(error), clock),
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

pub fn main() {
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
    let http_threads = start_http_server(&opts, daemon.metrics_snapshot.clone());

    // Start fetching prices, but only if fetching is enabled. If it is, this
    // never exits.
    if !opts.read_only {
        daemon.run();
    }

    // These threads never exit, so this blocks indefinitely.
    for thread in http_threads {
        thread
            .join()
            .expect("We don't observe thread panics, we set panic=abort.")
    }
}

#[cfg(test)]
mod test {
    use tiny_http::HeaderField;

    use super::*;

    #[test]
    fn test_get_average_apy() {
        let conn = Connection::open_in_memory().expect("Failed to open sqlite connection.");
        create_db(&conn).unwrap();
        let exchange_rate = ExchangeRate {
            id: 0,
            timestamp: chrono::Utc.ymd(2020, 8, 8).and_hms(0, 0, 0),
            slot: 116640000 + 1000, // First slot for epoch 270: 116640000
            epoch: 270,
            pool: SOLIDO_ID.to_owned(),
            price_lamports_numerator: 1,
            price_lamports_denominator: 1,
        };
        insert_price(&conn, &exchange_rate).unwrap();
        let exchange_rate = ExchangeRate {
            id: 0,
            timestamp: chrono::Utc.ymd(2021, 1, 8).and_hms(0, 0, 0),
            slot: 117072000 + 1000, // First slot for epoch 271: 117072000
            epoch: 271,
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
        assert_eq!(apy.unwrap().annual_percentage_yield(), 4.7989255185326485);
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
            slot: 116640000 + 1000, // First slot for epoch 270: 116640000
            epoch: 270,
            pool: SOLIDO_ID.to_owned(),
            price_lamports_numerator: 1936245653069130,
            price_lamports_denominator: 1893971837707973,
        };
        insert_price(&conn, &exchange_rate).unwrap();

        let exchange_rate = ExchangeRate {
            id: 0,
            timestamp: chrono::Utc.ymd(2022, 02, 28).and_hms(11, 58, 39),
            slot: 117072000 + 1000, // First slot for epoch 271: 117072000
            epoch: 271,
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
    fn test_get_interval_price_for_period_slot_bounds_do_not_overflow() {
        let conn = Connection::open_in_memory().expect("Failed to open sqlite connection.");
        create_db(&conn).unwrap();

        // We insert some bogus exchange rates here with timestamps and epoch/slot
        // combinations that will not occur in practice, but we still cover the
        // case to ensure we don't have overflows. The important thing here is
        // that we set a large epoch number, such that the slot number is going
        // to be close to `u64::MAX`, and adding `QUERY_SLOT_OFFSET` overflows.
        let exchange_rate = ExchangeRate {
            id: 0,
            timestamp: chrono::Utc.ymd(2022, 1, 1).and_hms(0, 0, 0),
            slot: 1,
            epoch: (i64::MAX - 1) as u64,
            pool: SOLIDO_ID.to_owned(),
            price_lamports_numerator: 100,
            price_lamports_denominator: 100,
        };
        insert_price(&conn, &exchange_rate).unwrap();

        let exchange_rate = ExchangeRate {
            id: 0,
            timestamp: chrono::Utc.ymd(2022, 2, 1).and_hms(0, 0, 0),
            slot: 2,
            epoch: i64::MAX as u64,
            pool: SOLIDO_ID.to_owned(),
            price_lamports_numerator: 100,
            price_lamports_denominator: 100,
        };
        insert_price(&conn, &exchange_rate).unwrap();

        let interval_price = get_interval_price_for_period(
            conn.unchecked_transaction().unwrap(),
            chrono::Utc.ymd(2022, 1, 1).and_hms(0, 0, 0),
            chrono::Utc.ymd(2022, 1, 1).and_hms(0, 0, 0),
            SOLIDO_ID.to_owned(),
        )
        .expect("Getting interval price should not fail with a SQL error.");
        assert!(interval_price.is_none());
    }

    #[test]
    fn test_get_date_from_url_parameters() {
        use url::form_urlencoded;

        let query_params = form_urlencoded::Serializer::new(String::new())
            .append_pair("begin", "2022-02-04T11:40:02.683960+00:00")
            .append_pair("end", "2022-02-07T14:22:08.826526+00:00")
            .finish();

        let now = chrono::Utc::now();

        let dates = get_date_params(
            form_urlencoded::parse(query_params.as_bytes()).collect::<Vec<_>>(),
            now,
        );
        assert_eq!(
            dates,
            Ok(DateRequest {
                date_range: parse_utc_iso8601("2022-02-04T11:40:02.683960+00:00").unwrap()
                    ..parse_utc_iso8601("2022-02-07T14:22:08.826526+00:00").unwrap(),
                request_type: DateRequestType::Fixed
            })
        );
    }

    #[test]
    fn test_get_date_from_url_parameters_days() {
        use url::form_urlencoded;

        let query_params = form_urlencoded::Serializer::new(String::new())
            .append_pair("days", "2")
            .finish();

        let now = parse_utc_iso8601("2022-02-07T11:40:02.683960+00:00").unwrap();

        let dates = get_date_params(
            form_urlencoded::parse(query_params.as_bytes()).collect::<Vec<_>>(),
            now,
        );

        assert_eq!(
            dates,
            Ok(DateRequest {
                date_range: parse_utc_iso8601("2022-02-05T11:40:02.683960+00:00").unwrap()
                    ..parse_utc_iso8601("2022-02-07T11:40:02.683960+00:00").unwrap(),
                request_type: DateRequestType::MovingTarget
            })
        );
    }

    #[test]
    fn test_get_date_from_url_parameters_since_launch() {
        use url::form_urlencoded;

        let query_params = form_urlencoded::Serializer::new(String::new())
            .append_key_only("since_launch")
            .finish();

        let now = parse_utc_iso8601("2022-02-07T11:40:02.683960+00:00").unwrap();

        let dates = get_date_params(
            form_urlencoded::parse(query_params.as_bytes()).collect::<Vec<_>>(),
            now,
        );

        assert_eq!(
            dates,
            Ok(DateRequest {
                date_range: parse_utc_iso8601("2021-09-01T00:00:00+00:00").unwrap()
                    ..parse_utc_iso8601("2022-02-07T11:40:02.683960+00:00").unwrap(),
                request_type: DateRequestType::MovingTarget
            })
        );
    }

    #[test]
    fn test_get_single_point() {
        let conn = Connection::open_in_memory().expect("Failed to open sqlite connection.");
        create_db(&conn).unwrap();
        let exchange_rate = ExchangeRate {
            id: 0,
            timestamp: chrono::Utc.ymd(2022, 01, 28).and_hms(11, 58, 39),
            slot: 116643000,
            epoch: 270,
            pool: SOLIDO_ID.to_owned(),
            price_lamports_numerator: 1936245653069130,
            price_lamports_denominator: 1893971837707973,
        };
        insert_price(&conn, &exchange_rate).unwrap();

        let apy = get_interval_price_for_period(
            conn.unchecked_transaction().unwrap(),
            chrono::Utc.ymd(2020, 7, 7).and_hms(0, 0, 0),
            chrono::Utc.ymd(2022, 7, 8).and_hms(0, 0, 0),
            SOLIDO_ID.to_owned(),
        )
        .expect("Failed when getting APY for period");
        let growth_factor = apy.unwrap().annual_percentage_yield();
        assert_eq!(growth_factor, 0.);
    }

    #[test]
    fn test_get_none_when_no_data_point() {
        let conn = Connection::open_in_memory().expect("Failed to open sqlite connection.");
        create_db(&conn).unwrap();
        let apy = get_interval_price_for_period(
            conn.unchecked_transaction().unwrap(),
            chrono::Utc.ymd(2020, 7, 7).and_hms(0, 0, 0),
            chrono::Utc.ymd(2022, 7, 8).and_hms(0, 0, 0),
            SOLIDO_ID.to_owned(),
        )
        .expect("Failed when getting APY for period");
        assert_eq!(apy, None);
    }

    #[test]
    fn test_get_none_when_slot_too_small() {
        let conn = Connection::open_in_memory().expect("Failed to open sqlite connection.");
        create_db(&conn).unwrap();
        let exchange_rate = ExchangeRate {
            id: 0,
            timestamp: chrono::Utc.ymd(2022, 01, 28).and_hms(11, 58, 39),
            slot: 116640000 + 999, // First slot for epoch 270: 116640000
            epoch: 270,
            pool: SOLIDO_ID.to_owned(),
            price_lamports_numerator: 1936245653069130,
            price_lamports_denominator: 1893971837707973,
        };
        insert_price(&conn, &exchange_rate).unwrap();

        let apy = get_interval_price_for_period(
            conn.unchecked_transaction().unwrap(),
            chrono::Utc.ymd(2020, 7, 7).and_hms(0, 0, 0),
            chrono::Utc.ymd(2022, 7, 8).and_hms(0, 0, 0),
            SOLIDO_ID.to_owned(),
        )
        .unwrap();
        assert_eq!(apy, None);
    }

    #[test]
    fn test_expiration_header_past() {
        let begin_epoch_291_datetime = chrono::Utc.ymd(2022, 3, 19).and_hms(18, 20, 02);
        let begin_epoch_292_datetime = chrono::Utc.ymd(2022, 3, 22).and_hms(13, 28, 50);

        let begin_epoch_293_datetime = chrono::Utc.ymd(2022, 3, 25).and_hms(07, 43, 25);
        let first_slot_epoch_293 = 126_576_000;

        let interval_prices = IntervalPrices {
            begin_datetime: begin_epoch_291_datetime,
            end_datetime: begin_epoch_292_datetime,
            begin_epoch: 291,
            end_epoch: 292,
            begin_token_price_sol: Rational {
                numerator: 1,
                denominator: 1,
            },
            end_token_price_sol: Rational {
                numerator: 1,
                denominator: 1,
            },
        };
        let current_clock = Clock {
            slot: first_slot_epoch_293,
            epoch_start_timestamp: begin_epoch_293_datetime.timestamp(),
            epoch: 293,
            leader_schedule_epoch: 295,
            unix_timestamp: (begin_epoch_293_datetime + chrono::Duration::hours(8)).timestamp(),
        };

        let expiration_header = get_expires_header(
            begin_epoch_293_datetime,
            &interval_prices,
            Some(current_clock),
            &DateRequestType::Fixed,
        );
        assert_eq!(
            expiration_header.field,
            HeaderField::from_str("Expires").unwrap()
        );
        let next_week_expiration = begin_epoch_293_datetime + chrono::Duration::weeks(1);
        assert_eq!(expiration_header.value, next_week_expiration.to_rfc2822());
    }

    #[test]
    fn test_expiration_header_present() {
        let begin_epoch_291_datetime = chrono::Utc.ymd(2022, 3, 19).and_hms(18, 20, 02);
        let begin_epoch_292_datetime = chrono::Utc.ymd(2022, 3, 22).and_hms(13, 28, 50);
        let first_slot_epoch_292 = 126_144_000;

        let interval_prices = IntervalPrices {
            begin_datetime: begin_epoch_291_datetime,
            end_datetime: begin_epoch_292_datetime,
            begin_epoch: 291,
            end_epoch: 292,
            begin_token_price_sol: Rational {
                numerator: 1,
                denominator: 1,
            },
            end_token_price_sol: Rational {
                numerator: 1,
                denominator: 1,
            },
        };

        let current_clock = Clock {
            slot: first_slot_epoch_292,
            epoch_start_timestamp: begin_epoch_292_datetime.timestamp(),
            epoch: 292,
            leader_schedule_epoch: 295,
            unix_timestamp: (begin_epoch_292_datetime + chrono::Duration::hours(8)).timestamp(),
        };

        let expiration_header = get_expires_header(
            begin_epoch_292_datetime,
            &interval_prices,
            Some(current_clock),
            &DateRequestType::MovingTarget,
        );
        assert_eq!(
            expiration_header.field,
            HeaderField::from_str("Expires").unwrap()
        );

        let expected_time =
            chrono::DateTime::parse_from_rfc3339("2022-03-24T19:11:52.399808+00:00").unwrap();
        assert_eq!(expiration_header.value, expected_time.to_rfc2822());
    }

    #[test]
    fn test_expiration_header_short_time() {
        let begin_epoch_291_datetime = chrono::Utc.ymd(2022, 3, 19).and_hms(18, 20, 02);
        let begin_epoch_292_datetime = chrono::Utc.ymd(2022, 3, 22).and_hms(13, 28, 50);

        let begin_epoch_293_datetime = chrono::Utc.ymd(2022, 3, 25).and_hms(07, 43, 25);
        let first_slot_epoch_293 = 126_576_000;

        let interval_prices = IntervalPrices {
            begin_datetime: begin_epoch_291_datetime,
            end_datetime: begin_epoch_292_datetime,
            begin_epoch: 291,
            end_epoch: 292,
            begin_token_price_sol: Rational {
                numerator: 1,
                denominator: 1,
            },
            end_token_price_sol: Rational {
                numerator: 1,
                denominator: 1,
            },
        };

        let epoch_293_minus_1m_datetime = begin_epoch_293_datetime - chrono::Duration::minutes(1);
        let current_clock = Clock {
            slot: first_slot_epoch_293 - 10,
            epoch_start_timestamp: begin_epoch_292_datetime.timestamp(),
            epoch: 292,
            leader_schedule_epoch: 295,
            unix_timestamp: epoch_293_minus_1m_datetime.timestamp(),
        };

        let expiration_header = get_expires_header(
            epoch_293_minus_1m_datetime,
            &interval_prices,
            Some(current_clock),
            &DateRequestType::MovingTarget,
        );
        assert_eq!(
            expiration_header.field,
            HeaderField::from_str("Expires").unwrap()
        );

        let expected_time = epoch_293_minus_1m_datetime + chrono::Duration::minutes(5);
        assert_eq!(expiration_header.value, expected_time.to_rfc2822());
    }
}
