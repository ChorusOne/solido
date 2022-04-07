// SPDX-FileCopyrightText: 2022 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

#![no_main]

use std::sync::{Arc, Mutex};

use arbitrary::Arbitrary;
use chrono;
use libfuzzer_sys::fuzz_target;
use rusqlite::Connection;
use solana_sdk::clock::{Clock, Epoch, Slot};
use tiny_http::TestRequest;

use listener::ExchangeRate;

/// Used to generate an arbitrary `solana_sdk::clock::Clock`. There may be some
/// silent undocumented assumptions in the Solana clock, but we don't respect
/// those here ... let's see if it breaks anything if we make a weird clock
/// (like one where the timestamp is lower than the start timestamp).
#[derive(Arbitrary, Debug)]
struct ClockFields {
    slot: u64,
    epoch_start_timestamp: i64,
    epoch: u64,
    leader_schedule_epoch: u64,
    unix_timestamp: i64,
}

#[derive(Arbitrary, Debug)]
enum Action {
    Insert {
        // These fields match lib::ExchangeRate. We don't #[derive(Arbitrary)]
        // in there, because we can't derive it for DateTime, we need to do that
        // manually here.
        timestamp_millis: i64,
        slot: Slot,
        epoch: Epoch,
        pool: String,
        price_lamports_numerator: u64,
        price_lamports_denominator: u64,
    },
    Request {
        path: String,
        clock_fields: Option<ClockFields>,
    },
}

fuzz_target!(|actions: Vec<Action>| {
    use chrono::TimeZone;
    let conn = Connection::open_in_memory().expect("Failed to open sqlite connection.");
    listener::create_db(&conn).unwrap();

    for action in &actions {
        match action {
            Action::Insert {
                timestamp_millis,
                slot,
                epoch,
                pool,
                price_lamports_numerator,
                price_lamports_denominator,
            } => {
                let timestamp = match chrono::Utc.timestamp_millis_opt(*timestamp_millis) {
                    chrono::LocalResult::Single(t) => t,
                    _ => continue,
                };
                let exchange_rate = ExchangeRate {
                    id: 0, // id is not used for inserts, only for reads.
                    timestamp: timestamp,
                    slot: *slot,
                    epoch: *epoch,
                    pool: pool.clone(),
                    price_lamports_numerator: *price_lamports_numerator,
                    price_lamports_denominator: *price_lamports_denominator,
                };
                // The insert can still fail if the values are not in bounds for
                // SQLite's limits, but that is not what we are fuzzing for here,
                // so ignore those errors.
                let _ = listener::insert_price(&conn, &exchange_rate);
            }
            Action::Request { path, clock_fields } => {
                let request = TestRequest::new().with_path(path).into();
                let metrics = listener::Metrics {
                    polls: 0,
                    errors: 0,
                    solido_average_30d_interval_price: None,
                };
                let clock = clock_fields.as_ref().map(|f| Clock {
                    slot: f.slot,
                    epoch_start_timestamp: f.epoch_start_timestamp,
                    epoch: f.epoch,
                    leader_schedule_epoch: f.leader_schedule_epoch,
                    unix_timestamp: f.unix_timestamp,
                });
                let snapshot = listener::Snapshot { metrics, clock };
                let metrics_mutex = Mutex::new(Arc::new(snapshot));
                listener::serve_request(&conn, request, &metrics_mutex).unwrap();
            }
        }
    }
});
