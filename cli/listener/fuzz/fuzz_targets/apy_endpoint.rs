// SPDX-FileCopyrightText: 2022 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

#![no_main]

use std::sync::{Arc, Mutex};

use arbitrary::Arbitrary;
use chrono;
use libfuzzer_sys::fuzz_target;
use rusqlite::Connection;
use solana_sdk::{clock::{Epoch, Slot}};
use tiny_http::TestRequest;

use listener::ExchangeRate;

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
    },
}

fuzz_target!(|actions: Vec<Action>| {
    use chrono::TimeZone;
    let conn = Connection::open_in_memory().expect("Failed to open sqlite connection.");
    listener::create_db(&conn).unwrap();

    for action in &actions {
        match action {
            Action::Insert {
                timestamp_millis, slot, epoch, pool, price_lamports_numerator, price_lamports_denominator,
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
            Action::Request {
                path
            } => {
                let request = TestRequest::new().with_path(path).into();
                let metrics = listener::Metrics {
                    polls: 0,
                    errors: 0,
                    solido_average_30d_interval_price: None
                };
                let snapshot = listener::Snapshot {
                    metrics: metrics,
                    // TODO: Fuzz with clock as well.
                    clock: None,
                };
                let metrics_mutex = Mutex::new(Arc::new(snapshot));
                listener::serve_request(&conn, request, &metrics_mutex);
            }
        }
    }
});
