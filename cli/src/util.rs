use std::fmt;

use serde::{Serialize, Serializer};
use solana_client::client_error::{ClientError, ClientErrorKind};
use solana_client::rpc_request::{RpcError, RpcResponseErrorData};
use solana_program::pubkey::Pubkey;
use solana_sdk::transaction::TransactionError;

use spl_stake_pool::solana_program::program_error::ProgramError;
use spl_stake_pool::solana_program::pubkey::PubkeyError;

/// Wrapper for `Pubkey` to serialize it as base58 in json, instead of a list of numbers.
pub struct PubkeyBase58(pub Pubkey);

impl fmt::Display for PubkeyBase58 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Serialize for PubkeyBase58 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Defer to the Display impl, which formats as base58.
        serializer.collect_str(&self.0)
    }
}

impl From<Pubkey> for PubkeyBase58 {
    fn from(pk: Pubkey) -> PubkeyBase58 {
        PubkeyBase58(pk)
    }
}

impl From<&Pubkey> for PubkeyBase58 {
    fn from(pk: &Pubkey) -> PubkeyBase58 {
        PubkeyBase58(*pk)
    }
}

/// Print the message in bold using ANSI escape sequences.
fn print_key(message: &'static str) {
    // 1m enters bold, 0m is a reset.
    // Format left-aligned with a minimum width of 11.
    print!("  \x1b[1m{:<11}\x1b[0m", message);
}

/// Print the message in red using ANSI escape sequences.
fn print_red(message: &'static str) {
    // 31m enters red, 0m is a reset.
    print!("\x1b[31m{}\x1b[0m", message);
}

pub trait AsPrettyError {
    /// Pretty-print the error.
    fn print_pretty(&self);
}

impl AsPrettyError for ClientError {
    fn print_pretty(&self) {
        print_red("Solana RPC client returned an error:\n\n");
        print_key("Request:");
        println!(" {:?}", self.request());
        print_key("Kind:");
        match self.kind() {
            ClientErrorKind::Io(inner) => {
                println!(" IO error\n\n{:?}", inner);
            }
            ClientErrorKind::Reqwest(inner) => {
                println!(" \"Reqwest\" error\n\n{:?}", inner);
            }
            ClientErrorKind::RpcError(inner) => match inner {
                RpcError::RpcRequestError(message) => {
                    println!(" RPC request error\n  {}", message)
                }
                RpcError::RpcResponseError {
                    code,
                    message,
                    data,
                } => {
                    println!(" RPC response error");
                    print_key("Error code:");
                    println!(" {}", code);
                    print_key("Message:");
                    println!(" {}", message);
                    match data {
                        RpcResponseErrorData::Empty => {}
                        RpcResponseErrorData::SendTransactionPreflightFailure(result) => {
                            print_key("Reason:");
                            println!(" Transaction preflight failure");
                            print_key("Error:");
                            println!(" {:?}", result.err);
                            print_key("Logs:");
                            match result.logs {
                                None => {
                                    println!(" unavailable");
                                }
                                Some(ref lines) => {
                                    println!("\n");
                                    for line in lines {
                                        println!("    {}", line);
                                    }
                                }
                            }
                        }
                        RpcResponseErrorData::NodeUnhealthy { num_slots_behind } => {
                            print_key("Reason:");
                            println!(" Node unhealthy, {:?} slots behind", num_slots_behind);
                        }
                    }
                }
                RpcError::ParseError(message) => {
                    println!(" RPC parse error\n  {}", message)
                }
                RpcError::ForUser(message) => {
                    println!(" RPC error for user\n  {}", message)
                }
            },
            ClientErrorKind::SerdeJson(inner) => {
                println!(" Serialization error\n\n{:?}", inner);
            }
            ClientErrorKind::SigningError(inner) => {
                println!(" Signing error\n\n{:?}", inner);
            }
            ClientErrorKind::TransactionError(inner) => {
                println!(" Transaction error\n\n{:?}", inner);
            }
            ClientErrorKind::FaucetError(inner) => {
                println!(" Faucet error\n\n{:?}", inner);
            }
            ClientErrorKind::Custom(message) => {
                println!(" Custom error\n  {}", message);
            }
        }
    }
}

impl AsPrettyError for ProgramError {
    fn print_pretty(&self) {
        println!("TODO: Add a nicer print_pretty impl for ProgramError.");
        println!("Program error:\n{:?}", self);
    }
}

impl AsPrettyError for TransactionError {
    fn print_pretty(&self) {
        println!("TODO: Add a nicer print_pretty impl for TransactionError.");
        println!("Transaction error:\n{:?}", self);
    }
}

impl AsPrettyError for std::io::Error {
    fn print_pretty(&self) {
        print_red("IO Error:");
        println!(" {:?}", self);
    }
}

impl AsPrettyError for bincode::ErrorKind {
    fn print_pretty(&self) {
        print_red("Bincode (de)serialization error:");
        println!(" {:?}", self);
    }
}

impl AsPrettyError for PubkeyError {
    fn print_pretty(&self) {
        print_red("Solana public key error:");
        println!(" {:?}", self);
    }
}

impl AsPrettyError for Box<dyn AsPrettyError + 'static> {
    fn print_pretty(&self) {
        (*self).print_pretty()
    }
}

pub trait Abort {
    type Item;

    /// If the result is an error, pretty-print and abort, otherwise return the `Ok`.
    fn ok_or_abort(self) -> Self::Item;

    fn ok_or_abort_with(self, message: &'static str) -> Self::Item;
}

impl<T, E: AsPrettyError> Abort for Result<T, E> {
    type Item = T;

    fn ok_or_abort(self) -> T {
        match self {
            Ok(result) => result,
            Err(err) => {
                err.print_pretty();
                std::process::exit(1);
            }
        }
    }

    fn ok_or_abort_with(self, message: &'static str) -> T {
        println!("{}", message);
        self.ok_or_abort()
    }
}
