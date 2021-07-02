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

use anchor_lang::AccountDeserialize;
use solana_client::rpc_client::RpcClient;
use solana_sdk::account::Account;
use solana_sdk::borsh::try_from_slice_unchecked;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_sdk::sysvar::stake_history::StakeHistory;
use solana_sdk::sysvar::{
    self, clock::Clock, recent_blockhashes::RecentBlockhashes, rent::Rent, Sysvar,
};
use solana_sdk::transaction::Transaction;

use lido::state::Lido;
use lido::token::Lamports;
use spl_token::solana_program::hash::Hash;

use crate::error::{Error, MissingAccountError, SerializationError};
use solana_client::client_error::{ClientErrorKind, ClientError};
use solana_client::rpc_request::RpcError;

pub enum SnapshotError {
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

impl<T> From<T> for SnapshotError
where
    Error: From<T>,
{
    fn from(err: T) -> SnapshotError {
        SnapshotError::OtherError(Error::from(err))
    }
}

pub type Result<T> = std::result::Result<T, SnapshotError>;

/// A snapshot of one or more accounts.
pub struct Snapshot<'a> {
    /// Addresses, and their values, at the time of the snapshot.
    accounts: &'a HashMap<Pubkey, Account>,

    /// The accounts that we actually read. This is used to remove any unused
    /// accounts from a future snapshot, so the set of accounts to query does
    /// not continue growing indefinitely.
    accounts_referenced: &'a mut HashSet<Pubkey>,

    /// The wrapped client, so we can still send transactions.
    rpc_client: &'a RpcClient,

    /// Whether we sent at least one transaction.
    ///
    /// If we did, then retrying is potentially unsafe, because it would also
    /// retry sending the transaction. If that happens, we need to update the
    /// program, so it reads everything it needs before sending a transaction.
    sent_transaction: &'a mut bool,
}

impl<'a> Snapshot<'a> {
    pub fn get_account(&mut self, address: &Pubkey) -> Result<&'a Account> {
        self.accounts_referenced.insert(*address);
        self.accounts
            .get(address)
            .ok_or(SnapshotError::MissingAccount)
    }

    /// Read an account and immediately bincode-deserialize it.
    pub fn get_bincode<T: Sysvar>(&mut self, address: &Pubkey) -> Result<T> {
        let account = self.get_account(address)?;
        let result = bincode::deserialize(&account.data)?;
        Ok(result)
    }

    /// Read an Anchor account and immediately deserialize it.
    pub fn get_account_deserialize<T: AccountDeserialize>(
        &mut self,
        address: &Pubkey,
    ) -> Result<T> {
        let account = self.get_account(address)?;
        let mut data_ref = &account.data[..];
        let result = T::try_deserialize(&mut data_ref)?;
        Ok(result)
    }

    /// Read `sysvar::rent`.
    pub fn get_rent(&mut self) -> Result<Rent> {
        self.get_bincode(&sysvar::rent::id())
    }

    /// Read `sysvar::clock`.
    pub fn get_clock(&mut self) -> Result<Clock> {
        self.get_bincode(&sysvar::clock::id())
    }

    /// Read `sysvar::stake_history`.
    pub fn get_stake_history(&mut self) -> Result<StakeHistory> {
        self.get_bincode(&sysvar::stake_history::id())
    }

    /// Read `sysvar::recent_blockhashes`.
    pub fn get_recent_blockhashes(&mut self) -> Result<RecentBlockhashes> {
        self.get_bincode(&sysvar::recent_blockhashes::id())
    }

    /// Return the most recent block hash at the time of the snapshot.
    pub fn get_recent_blockhash(&mut self) -> Result<Hash> {
        let blockhashes = self.get_recent_blockhashes()?;
        // The blockhashes are ordered from most recent to least recent.
        Ok(blockhashes[0].blockhash)
    }

    /// Return the minimum rent-exempt balance for an account with `data_len` bytes of data.
    pub fn get_minimum_balance_for_rent_exemption(&mut self, data_len: usize) -> Result<Lamports> {
        let rent = self.get_rent()?;
        Ok(Lamports(rent.minimum_balance(data_len)))
    }

    /// Read the account and deserialize the Solido struct.
    pub fn get_solido(&mut self, solido_address: &Pubkey) -> Result<Lido> {
        let account = self.get_account(solido_address)?;
        match try_from_slice_unchecked::<Lido>(&account.data) {
            Ok(solido) => Ok(solido),
            Err(err) => {
                let error: Error = Box::new(SerializationError {
                    cause: err.into(),
                    address: *solido_address,
                    context: format!("Failed to deserialize Lido struct, data length is {} bytes.", account.data.len()),
                });
                Err(error.into())
            }
        }
    }

    /// Send a transaction without printing to stdout.
    ///
    /// After this, avoid reads from accounts not accessed before. Note, you
    /// probably want to use [`SnapshotConfig::sign_and_send_transaction`]
    /// instead of calling this directly, to ensure correct output handling.
    pub fn send_and_confirm_transaction(
        &mut self,
        transaction: &Transaction,
    ) -> solana_client::client_error::Result<Signature> {
        *self.sent_transaction = true;
        self.rpc_client.send_and_confirm_transaction(transaction)
    }

    /// Send a transaction, show a spinner on stdout.
    ///
    /// After this, avoid reads from accounts not accessed before. Note, you
    /// probably want to use [`SnapshotConfig::sign_and_send_transaction`]
    /// instead of calling this directly, to ensure correct output handling.
    pub fn send_and_confirm_transaction_with_spinner(
        &mut self,
        transaction: &Transaction,
    ) -> solana_client::client_error::Result<Signature> {
        *self.sent_transaction = true;
        self.rpc_client
            .send_and_confirm_transaction_with_spinner(transaction)
    }
}

/// A wrapper around [`RpcClient`] that enables reading consistent snapshots of multiple accounts.
pub struct SnapshotClient {
    rpc_client: RpcClient,
    accounts_to_query: Vec<Pubkey>,
}

/// Return whether a call to `GetMultipleAccounts` failed due to the RPC account limit.
///
/// If this happens, the RPC operator increase `--rpc-max-multiple-accounts` on
/// their validator. At the time of writing, it defaults to 100.
fn is_too_many_inputs_error(error: &ClientError) -> bool {
    match error.kind() {
        ClientErrorKind::RpcError(inner) => match inner {
            // Unfortunately, there is no way to get a structured error; all we
            // get is a string that looks like this:
            //
            //     Failed to deserialize RPC error response: {"code":-32602,
            //     "message":"Too many inputs provided; max 100"} [missing field `data`]
            //
            // So we have to resort to testing for a substring, and if Solana
            // ever changes their responses, this will break :/
            RpcError::RpcRequestError(message) => message.contains("Too many inputs provided"),
            _ => false,
        }
        _ => false,
    }
}

impl SnapshotClient {
    pub fn new(rpc_client: RpcClient) -> SnapshotClient {
        SnapshotClient {
            rpc_client,
            accounts_to_query: Vec::new(),
        }
    }

    /// Call `GetMultipleAccounts` to get `self.accounts_to_query`.
    ///
    /// Ideally, we do a single `GetMultipleAccounts` call for the accounts we
    /// need, and then we have a consistent snapshot. But unfortunately, the
    /// default limit on the number of accounts that you can query in one call
    /// is quite low. This means that in somme cases, we may need to resort to
    /// doing multiple calls. This can result in torn reads, and observing an
    /// inconsistent state, but unfortunately there is no other way. If this
    /// happens, we print a warning to stderr.
    fn get_multiple_accounts_chunked(&self) -> std::result::Result<Vec<Option<Account>>, crate::error::Error> {
        let mut result = Vec::new();

        // Handle the empty case first, because otherwise we try to make chunks
        // of length 0 below.
        if self.accounts_to_query.is_empty() {
            return Ok(result);
        }

        'num_chunks: for num_chunks in 1.. {
            result.clear();

            let items_per_chunk = self.accounts_to_query.len() / num_chunks;
            assert!(
                items_per_chunk > 0,
                "We should be able to get at least *one* account with GetMultipleAccounts."
            );

            for chunk in self.accounts_to_query.chunks(items_per_chunk) {
                match self.rpc_client.get_multiple_accounts(chunk) {
                    Ok(accounts) => {
                        result.extend(accounts);
                    }
                    Err(ref err) if is_too_many_inputs_error(err) => {
                        eprintln!(
                            "Warning: Failed to retrieve all accounts in a single \
                                GetMultipleAccounts call. The resulting snapshot may be \
                                inconsistent."
                        );
                        eprintln!(
                            "Please ask the RPC node operator to bump \
                                --rpc-max-multiple-accounts to {}, or connect to a different RPC \
                                node.",
                            self.accounts_to_query.len()
                        );
                        continue 'num_chunks;
                    }
                    Err(err) => return Err(err.into()),
                };
            }

            assert_eq!(result.len(), self.accounts_to_query.len());
            return Ok(result);
        }

        unreachable!("Above loop fails the assertion when items_per_chunk > accounts_to_query.len");
    }

    /// Run the function `f`, which has access to a consistent snapshot of accounts.
    ///
    /// If `f` tries to access an account that's not in the snapshot, we will
    /// retry with an extended snapshot. This means that `f` can be called
    /// multiple times, beware of side effects! In particular, after sending a
    /// transaction, `f` should not try to access any accounts that it did not
    /// access before sending the transaction. For sending transactions, this
    /// function will detect that and panic, but for external side effects (such
    /// as printing to stdout), we can’t, so be careful.
    ///
    /// For the first iteration, the accounts that we load are the ones from the
    /// previous call. This means that it's better to recycle one snapshot client,
    /// than to create a new one all the time.
    pub fn with_snapshot<T, F>(&mut self, mut f: F) -> std::result::Result<T, crate::error::Error>
    where
        F: FnMut(Snapshot) -> Result<T>,
    {
        loop {
            let account_values = self.get_multiple_accounts_chunked()?;
            let accounts: HashMap<_, _> = self
                .accounts_to_query
                .iter()
                .zip(account_values)
                // `get_multiple_accounts` returns None for non-existing accounts,
                // filter those out.
                .filter_map(|(k, opt_v)| opt_v.map(|v| (*k, v)))
                .collect();

            // Confirm that we did read all the accounts that we needed, and
            // fail otherwise. Without this check, we could get stuck in an
            // infinite loop, trying to read the same non-existing account.
            for addr in &self.accounts_to_query {
                if !accounts.contains_key(addr) {
                    return Err(Box::new(MissingAccountError {
                        missing_account: *addr,
                    }));
                }
            }

            let mut accounts_referenced = HashSet::new();
            let mut sent_transaction = false;

            let snapshot = Snapshot {
                accounts: &accounts,
                accounts_referenced: &mut accounts_referenced,
                rpc_client: &self.rpc_client,
                sent_transaction: &mut sent_transaction,
            };

            match f(snapshot) {
                Ok(result) => return Ok(result),
                Err(SnapshotError::OtherError(err)) => return Err(err),
                Err(SnapshotError::MissingAccount) => {
                    if sent_transaction {
                        // `f` tried to access an account that was not in the
                        // snapshot, after already sending a transaction. We
                        // can't just retry, because it would also send that
                        // transaction again. This is a programming error.
                        panic!(
                            "Tried to read an account that is not in the snapshot, \
                            after sending a transaction. Move the read before the \
                            write, or make a new snapshot after the write."
                        );
                    } else {
                        // `f` tried to access an account that was not in the snapshot.
                        // That should have put the account in `accounts_referenced`,
                        // so on the next iteration, we will include that account.
                        self.accounts_to_query.clear();
                        self.accounts_to_query.extend(accounts_referenced);
                    }
                }
            }
        }
    }
}
