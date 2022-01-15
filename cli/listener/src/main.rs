use std::time::SystemTime;

use chrono::TimeZone;
use clap::Clap;
use lido::{
    state::Lido,
    token::{ArithmeticError, Rational},
};
use rusqlite::{params, Connection, Row};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    clock::{Epoch, Slot},
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::Keypair,
};
use solido_cli_common::snapshot::{
    Config, OutputMode, SnapshotClient, SnapshotConfig, SnapshotError,
};

#[derive(Clap, Debug)]
pub struct Opts {
    /// URL of cluster to connect to (e.g., https://api.devnet.solana.com for solana devnet)
    // Overwritten by `GeneralOpts` if None.
    #[clap(long, default_value = "http://127.0.0.1:8899")]
    cluster: String,

    /// Whether to output text or json. [default: "text"]
    // Overwritten by `GeneralOpts` if None.
    #[clap(long = "output", possible_values = &["text", "json"])]
    output_mode: Option<OutputMode>,

    /// Unique name for identifying
    #[clap(long, default_value = "solido")]
    pool: String,

    /// Poll frequency in seconds, defaults to 5 minutes.
    #[clap(long, default_value = "300")]
    poll_frequency_seconds: u32,

    /// Location of the SQLite DB file.
    #[clap(long, default_value = "listener.db")]
    db_path: String,
}

struct State {
    pub solido: Lido,
}

impl State {
    pub fn new(
        config: &mut SnapshotConfig,
        solido_program_id: &Pubkey,
        solido_address: &Pubkey,
    ) -> Result<Self, SnapshotError> {
        let solido = config.client.get_solido(solido_address)?;
        Ok(State { solido })
    }
}

#[derive(Debug)]
pub struct ExchangeRate {
    /// Id of the data point.
    id: i32,
    /// Unix timestamp when the data point was logged.
    timestamp: chrono::DateTime<chrono::Utc>,
    /// Slot when the data point was logged.
    slot: Slot,
    /// Epoch when the data point was logged.
    epoch: Epoch,
    /// Pool identifier, e.g. for Solido would be "solido".
    pool: String,
    /// Token_a price.
    token_a: u64,
    /// Token_b price.
    token_b: u64,
}

pub fn create_db(conn: &Connection) -> rusqlite::Result<()> {
    // The timestamp is stored in ISO-8601 format.
    conn.execute(
        "CREATE TABLE IF NOT EXISTS exchange_rate (
                id          INTEGER PRIMARY KEY,
                timestamp   TEXT,
                slot        INTEGER NOT NULL,
                epoch       INTEGER NOT NULL,
                pool        TEXT NOT NULL,
                token_a     INTEGER NOT NULL,
                token_b     INTEGER NOT NULL
            )",
        [],
    )?;
    Ok(())
}

pub struct IntervalPrices {
    t0: chrono::DateTime<chrono::Utc>,
    t1: chrono::DateTime<chrono::Utc>,
    epoch0: Epoch,
    epoch1: Epoch,
    price0_lamports: Rational,
    price1_lamports: Rational,
}

impl IntervalPrices {
    pub fn duration_wall_time(&self) -> chrono::Duration {
        self.t1 - self.t0
    }
    pub fn duration_epochs(&self) -> u64 {
        self.epoch1 - self.epoch0
    }
    pub fn growth_factor(&self) -> std::result::Result<Rational, ArithmeticError> {
        self.price1_lamports / self.price0_lamports
    }
    pub fn annual_growth_factor(&self) -> f64 {
        let year = chrono::Duration::days(365);
        self.growth_factor()
            .expect("Overflow happened when calculating growth factor.")
            .to_f64()
            .powf(year.num_seconds() as f64 / self.duration_wall_time().num_seconds() as f64)
    }
    pub fn annual_percentage_rate(&self) -> f64 {
        self.annual_growth_factor().mul_add(100.0, -100.0)
    }
}

pub fn get_apy_for_period(
    conn: &Connection,
    opts: &Opts,
    from_time: chrono::DateTime<chrono::Utc>,
    to_time: chrono::DateTime<chrono::Utc>,
) -> rusqlite::Result<Option<f64>> {
    let row_map = |row: &Row| {
        let timestamp_iso8601: String = row.get(1)?;
        Ok(ExchangeRate {
            id: row.get(0)?,
            timestamp: timestamp_iso8601
                .parse()
                .expect("Invalid timestamp format."),
            slot: row.get(2)?,
            epoch: row.get(3)?,
            pool: row.get(4)?,
            token_a: row.get(5)?,
            token_b: row.get(6)?,
        })
    };

    // Get first logged minimal logged data based on timestamp that is greater than `from_time`.
    let mut stmt_first = conn.prepare(
        "SELECT *, MIN(timestamp) FROM exchange_rate WHERE pool = :pool AND timestamp > :t",
    )?;
    let mut row_iter = stmt_first.query_map([opts.pool.clone(), from_time.to_string()], row_map)?;
    let first = row_iter.next();

    // Get first logged maximal logged data based on timestamp that is smaller than `to_time`.
    let mut stmt_last = conn.prepare(
        "SELECT *, MAX(timestamp) FROM exchange_rate WHERE pool = :pool AND timestamp < :t",
    )?;
    let mut row_iter = stmt_last.query_map([opts.pool.clone(), to_time.to_string()], row_map)?;
    let last = row_iter.next();

    match (first, last) {
        (Some(first), Some(last)) => {
            let first = first?;
            let last = last?;
            // Not enough data, need at least two data points.
            if first.id == last.id {
                Ok(None)
            } else {
                let interval_prices = IntervalPrices {
                    t0: first.timestamp,
                    t1: last.timestamp,
                    epoch0: first.epoch,
                    epoch1: last.epoch,
                    price0_lamports: Rational {
                        numerator: first.token_a,
                        denominator: first.token_b,
                    },
                    price1_lamports: Rational {
                        numerator: last.token_a,
                        denominator: last.token_b,
                    },
                };
                Ok(Some(interval_prices.annual_percentage_rate()))
            }
        }
        _ => Ok(None),
    }
    // Ok(Some(1.0))
}

pub fn insert_price(conn: &Connection, exchange_rate: ExchangeRate) -> rusqlite::Result<()> {
    conn.execute("INSERT INTO exchange_rate (timestamp, slot, epoch, pool, token_a, token_b) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", 
    params![exchange_rate.timestamp.to_string(), exchange_rate.slot, exchange_rate.epoch, exchange_rate.pool, exchange_rate.token_a, exchange_rate.token_b])?;
    Ok(())
}

fn main() {
    let opts = Opts::parse();
    solana_logger::setup_with_default("solana=info");
    let rpc_client = RpcClient::new_with_commitment(opts.cluster, CommitmentConfig::confirmed());
    let snapshot_client = SnapshotClient::new(rpc_client);

    let output_mode = opts.output_mode.unwrap();

    // Our config has a signer, which for this program we will not use, since we
    // only observe information from the Solana blockchain.
    let signer = Keypair::new();
    let config = Config {
        client: snapshot_client,
        signer: &signer,
        output_mode,
    };

    let conn = Connection::open(&opts.db_path).expect("Failed to open sqlite connection.");
    create_db(&conn).expect("Failed to create database.");
}

#[test]
fn test_get_average_apy() {
    let opts = Opts {
        cluster: "http://127.0.0.1:8899".to_owned(),
        output_mode: None,
        pool: "solido".to_owned(),
        poll_frequency_seconds: 1,
        db_path: "listener".to_owned(),
    };
    let conn = Connection::open_in_memory().expect("Failed to open sqlite connection.");
    create_db(&conn).unwrap();
    let exchange_rate = ExchangeRate {
        id: 0,
        timestamp: chrono::Utc.ymd(2020, 8, 8).and_hms(0, 0, 0),
        slot: 1,
        epoch: 1,
        pool: opts.pool.clone(),
        token_a: 1,
        token_b: 1,
    };
    insert_price(&conn, exchange_rate).unwrap();
    let exchange_rate = ExchangeRate {
        id: 0,
        timestamp: chrono::Utc.ymd(2021, 1, 8).and_hms(0, 0, 0),
        slot: 2,
        epoch: 2,
        pool: opts.pool.clone(),
        token_a: 1394458971361025,
        token_b: 1367327673971744,
    };
    insert_price(&conn, exchange_rate).unwrap();
    let apy = get_apy_for_period(
        &conn,
        &opts,
        chrono::Utc.ymd(2020, 7, 7).and_hms(0, 0, 0),
        chrono::Utc.ymd(2021, 7, 8).and_hms(0, 0, 0),
    )
    .expect("Failed when getting APY for period");
    assert_eq!(apy, Some(4.7989255185326485));
}
