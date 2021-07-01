//! Utilities for observing a consistent snapshot of on-chain state.
//!
//! The Solana RPC does not have any functionality to query an account at a given
//! block, but it can query multiple accounts at once. Therefore, this module
//! implements an opportunistic way of querying: read all accounts we *expect*
//! to need in one call. If that is all the accounts we really need, then great,
//! we have a consistent view of the on-chain state. If it turns out later that
//! we need to read from an account that is not in our snapshot, then adjust the
//! expected accounts, and retry.
//!
//! There are situations in which this could fail to ever get a useful snapshot.
//! For example, suppose we build a linked list of accounts, where the account's
//! data contains the address of the next account. We want to have a snapshot of
//! the list. If an external process keeps modifying the list, then every time
//! we get a new snapshot, we may find that the tail points to an account that
//! wasn’t yet included in the snapshot, so we retry. But by then, the external
//! process has already modified the tail again, so we are stuck in a loop.
//!
//! This is a pathological example though, for Solido we expect retries to be
//! rare, and when they do happen, they shouldn’t happen repeatedly.

use std::collections::{HashMap, HashSet};

use anchor_client::solana_sdk::account::Account;
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;

use crate::error::Error;

enum SnapshotError {
    /// We tried to access an account, but it was not present in the snapshot.
    ///
    /// When this happens, we need to retry, with a new set of accounts.
    MissingAccount,

    /// An error occurred that was not related to account lookup in the snapshot.
    ///
    /// When this happens, we need to abort trying to get the snapshot, and we
    /// just return this error.
    OtherError(Error),
}

pub struct Snapshot {
    /// Addresses, and their values, at the time of the snapshot.
    accounts: HashMap<Pubkey, Account>,

    /// The accounts that we actually read. This is used to remove any unused
    /// accounts from a future snapshot, so the set of accounts to query does
    /// not continue growing indefinitely.
    accounts_referenced: HashSet<Pubkey>,
}

impl Snapshot {
    pub fn get(&mut self, address: Pubkey) -> Result<&Account, SnapshotError> {
        self.accounts_referenced.insert(address);
        self.accounts
            .get(&address)
            .ok_or(SnapshotError::MissingAccount)
    }
}

/// A wrapper around [`RpcClient`] that enables reading consistent snapshots of multiple accounts.
pub struct SnapshotClient {
    rpc_client: RpcClient,
    accounts_to_query: Vec<Pubkey>,
}

impl SnapshotClient {
    pub fn new(rpc_client: RpcClient) -> SnapshotClient {
        SnapshotClient {
            rpc_client,
            accounts_to_query: Vec::new(),
        }
    }

    pub fn with_snapshot<T, F>(&mut self, f: F) -> Result<T, Error>
    where
        F: Fn(&mut Snapshot) -> Result<T, SnapshotError>,
    {
        loop {
            let account_values = self
                .rpc_client
                .get_multiple_accounts(&self.accounts_to_query[..])?;

            let accounts: HashMap<_, _> = self
                .accounts_to_query
                .iter()
                .zip(account_values)
                // `get_multiple_accounts` returns None for non-existing accounts,
                // filter those out.
                .filter_map(|(k, opt_v)| opt_v.map(|v| (*k, v)))
                .collect();

            let mut snapshot = Snapshot {
                accounts,
                accounts_referenced: HashSet::new(),
            };

            match f(&mut snapshot) {
                Ok(result) => return Ok(result),
                Err(SnapshotError::OtherError(err)) => return Err(err),
                Err(SnapshotError::MissingAccount) => {
                    // `f` tried to access an account that was not in the snapshot.
                    // That should have put the account in `accounts_referenced`,
                    // so on the next iteration, we will include that account.
                    self.accounts_to_query.clear();
                    self.accounts_to_query.extend(snapshot.accounts_referenced);
                }
            }
        }
    }
}
