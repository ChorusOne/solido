use clap::Clap;
use lido::state::Lido;
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

    /// Poll frequency in seconds, defaults to 86400s = 1 day.
    #[clap(long, default_value = "86400")]
    poll_frequency: u64,

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
    /// Unix timestamp when the data point was logged.
    timestamp: u64,
    /// Slot when the data point was logged.
    slot: Slot,
    /// Epoch when the data point was logged.
    epoch: Epoch,
    /// Pool identifier, e.g. for Solido would be "solido".
    pool: String,
    token_a: u64,
    token_b: u64,
}

pub fn create_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS exchange_rate (
                timestamp   INTEGER PRIMARY KEY,
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

pub fn get_average_apy(conn: &Connection, opts: &Opts) -> rusqlite::Result<Option<f32>> {
    let row_map = |row: &Row| {
        Ok(ExchangeRate {
            timestamp: row.get(0)?,
            slot: row.get(1)?,
            epoch: row.get(2)?,
            pool: row.get(3)?,
            token_a: row.get(4)?,
            token_b: row.get(5)?,
        })
    };

    // Get first logged entry by timestamp.
    let mut stmt_first = conn.prepare(
            "SELECT timestamp, slot, epoch, pool, token_a, token_b FROM exchange_rate WHERE pool = :pool ORDER BY timestamp ASC LIMIT 1",
        )?;
    let mut row_iter = stmt_first.query_map([opts.pool.clone()], row_map)?;
    let first = row_iter.next();

    // Get last logged entry by timestamp.
    let mut stmt_last = conn.prepare(
            "SELECT timestamp, slot, epoch, pool, token_a, token_b FROM exchange_rate WHERE pool = :pool ORDER BY timestamp DESC LIMIT 1",
        )?;
    let mut row_iter = stmt_last.query_map([opts.pool.clone()], row_map)?;
    let last = row_iter.next();

    match (first, last) {
        (Some(first), Some(last)) => {
            let exchange_rate_first = first?;
            let exchange_rate_last = last?;
            // Not enough data, need at least two data points.
            if exchange_rate_first.timestamp == exchange_rate_last.timestamp {
                Ok(None)
            } else {
                let seconds_in_year = 31536000f32;
                // Will not underflow because we order them in the query.
                let duration = exchange_rate_last.timestamp - exchange_rate_first.timestamp;
                let fraction_of_year = duration as f32 / seconds_in_year;
                let p0 = exchange_rate_first.token_a as f32 / exchange_rate_first.token_b as f32;
                let p1 = exchange_rate_last.token_a as f32 / exchange_rate_last.token_b as f32;
                let apy = (p1 / p0).powf(1. / (fraction_of_year));
                Ok(Some((apy - 1f32) * 100f32))
            }
        }
        _ => Ok(None),
    }
}

pub fn log_price(conn: &Connection, exchange_rate: ExchangeRate) -> rusqlite::Result<()> {
    conn.execute("INSERT INTO exchange_rate (timestamp, slot, epoch, pool, token_a, token_b) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", 
    params![exchange_rate.timestamp, exchange_rate.slot, exchange_rate.epoch, exchange_rate.pool, exchange_rate.token_a, exchange_rate.token_b])?;
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
        poll_frequency: 1,
        db_path: "listener".to_owned(),
    };
    let conn = Connection::open_in_memory().expect("Failed to open sqlite connection.");
    create_db(&conn).unwrap();
    let exchange_rate = ExchangeRate {
        timestamp: 1627839324,
        slot: 1,
        epoch: 1,
        pool: opts.pool.clone(),
        token_a: 1,
        token_b: 1,
    };
    log_price(&conn, exchange_rate).unwrap();
    let exchange_rate = ExchangeRate {
        timestamp: 1642008924,
        slot: 2,
        epoch: 2,
        pool: opts.pool.clone(),
        token_a: 1394458971361025,
        token_b: 1367327673971744,
    };
    log_price(&conn, exchange_rate).unwrap();
    let apy = get_average_apy(&conn, &opts).unwrap();
    assert_eq!(apy, Some(4.469979));
}
