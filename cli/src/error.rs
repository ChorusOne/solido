// SPDX-FileCopyrightText: 2021 Chorus One AG
// SPDX-License-Identifier: GPL-3.0

//! Error type for use throughout the CLI program and daemon.

use num_traits::cast::FromPrimitive;
use solana_client::client_error::{ClientError, ClientErrorKind};
use solana_client::rpc_request::{RpcError, RpcResponseErrorData};
use solana_program::instruction::InstructionError;
use solana_program::program_error::ProgramError;
use solana_program::pubkey::PubkeyError;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signer::presigner::PresignerError;
use solana_sdk::signer::SignerError;
use solana_sdk::transaction::TransactionError;

use anker::error::AnkerError;
use lido::error::LidoError;

/// Return whether the transaction may have executed despite the client error.
///
/// We observed one case on testnet where the RPC returned:
///
///     Solana RPC client returned an error:
///
///      Request:    None
///      Kind:       RPC error for user
///      unable to confirm transaction. This can happen in situations such as
///      transaction expiration and insufficient fee-payer funds
///
/// But the transaction had been executed nonetheless. I suspect what happened
/// is that sending the transaction succeeded, but something went wrong when
/// waiting for confirmations.
///
/// We check for this particular case so we can tell users to check if the
/// transaction executed before continuing. Note that when this function returns
/// true, there is no guarantee that the transaction did execute, and when it
/// returns false, there is no guarantee that it did not execute!
pub fn might_have_executed(error: &ClientError) -> bool {
    match error.kind() {
        ClientErrorKind::RpcError(RpcError::ForUser(message)) => {
            // Unfortunately the error is not more structured than this, we have
            // to string match on the message.
            message.contains("unable to confirm transaction.")
        }
        _ => false,
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

/// Trait for errors that can be printed to an ANSI terminal for human consumption.
pub trait AsPrettyError {
    /// Pretty-print the error.
    fn print_pretty(&self);
}

pub type Error = Box<dyn AsPrettyError + 'static>;

/// Something went wrong while performing maintenance.
pub struct MaintenanceError {
    pub message: String,
}

impl MaintenanceError {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(message: String) -> Error {
        Box::new(MaintenanceError { message })
    }
}

impl AsPrettyError for MaintenanceError {
    fn print_pretty(&self) {
        print_red("Maintenance error:\n\n");
        println!("{}", self.message);
    }
}

/// Something went wrong either while reading CLI arguments, or while using them.
///
/// This can be a user error (e.g. an invalid Ledger path), or it can be something
/// else that went wrong (e.g. connecting to the Ledger device).
pub struct CliError {
    pub message: &'static str,
    pub cause: Option<String>,
}

impl CliError {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(message: &'static str) -> Error {
        Box::new(CliError {
            message,
            cause: None,
        })
    }

    /// Create a new boxed error, including a cause.
    pub fn with_cause<E: ToString>(message: &'static str, cause: E) -> Error {
        Box::new(CliError {
            message,
            cause: Some(cause.to_string()),
        })
    }
}

impl AsPrettyError for CliError {
    fn print_pretty(&self) {
        print_red("Error:\n\n");
        print_key("Message:");
        println!("{}", self.message);
        if let Some(cause) = &self.cause {
            print_key("Cause:");
            println!("{}", cause);
        }
    }
}

/// We expected to read from the following account, but it doesn't exist on the network.
pub struct MissingAccountError {
    pub missing_account: Pubkey,
}

impl AsPrettyError for MissingAccountError {
    fn print_pretty(&self) {
        print_red("Missing account error:\n");
        println!(
            "We tried to read the following account, but it does not exist: {}",
            self.missing_account
        );
    }
}

/// We expected to read validator info for the given account, but it does not exist.
pub struct MissingValidatorInfoError {
    pub validator_identity: Pubkey,
}

impl AsPrettyError for MissingValidatorInfoError {
    fn print_pretty(&self) {
        print_red("Missing validator info error:\n");
        println!(
            "We tried to get the validator info for the validator with identity \
            account {}, but no validator info exists for this identity.",
            self.validator_identity
        );
    }
}

pub struct SerializationError {
    pub context: String,
    pub cause: Option<Error>,
    pub address: Pubkey,
}

impl AsPrettyError for SerializationError {
    fn print_pretty(&self) {
        print_red("Serialization error:\n\n");
        print_key("Context:");
        println!("{}", self.context);
        print_key("Address:");
        println!("{}", self.address);
        print_key("Cause:");
        match &self.cause {
            Some(cause) => cause.print_pretty(),
            None => println!("unspecified"),
        }
    }
}

fn print_pretty_transaction_error(err: &TransactionError) {
    // Indent all keys, because they are printed as part of a larger error.
    print_key("  Raw:    ");
    println!(" {:?}", err);
    print_key("  Display:");
    println!(" {}", err);

    if let TransactionError::InstructionError(_instr, ref instr_error) = err {
        println!();
        if let InstructionError::Custom(error_code) = instr_error {
            print_pretty_error_code(*error_code);
        }
    }
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
                println!(" \"Reqwest\" error");
                print_key("Message:");
                println!(" {}", inner);
                print_key("Raw:");
                println!(" {:#?}", inner);
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
                            match result.err {
                                Some(ref err) => {
                                    println!("\n");
                                    print_pretty_transaction_error(err);
                                    println!();
                                }
                                None => {
                                    println!(" unavailable");
                                }
                            }
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
            ClientErrorKind::TransactionError(ref inner) => {
                println!(" Transaction error");
                print_key("Error:");
                println!("\n");
                print_pretty_transaction_error(inner);
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

/// Parse an error code back to a multisig error.
///
/// We need to write this manually, because `multisig::Error::from`
/// unfortunately doesn’t convert back from error codes, it appears
/// to be broken. See also <https://github.com/ChorusOne/solido/issues/177>.
pub fn multisig_error_from_u32(error_code: u32) -> Option<serum_multisig::ErrorCode> {
    use serum_multisig::ErrorCode;

    let all_errors = [
        ErrorCode::InvalidOwner,
        ErrorCode::NotEnoughSigners,
        ErrorCode::TransactionAlreadySigned,
        ErrorCode::Overflow,
        ErrorCode::UnableToDelete,
        ErrorCode::AlreadyExecuted,
        ErrorCode::InvalidThreshold,
    ];

    // The purpose of the match statement below is to trigger a compile error if
    // the `ErrorCode` enum changes in a future version of the multisig program.
    // If you ended up here  to fix that, please also add the new variant to the
    // list above!
    match all_errors[0] {
        ErrorCode::InvalidOwner => { /* See comment above! */ }
        ErrorCode::NotEnoughSigners => { /* See comment above! */ }
        ErrorCode::TransactionAlreadySigned => { /* See comment above! */ }
        ErrorCode::Overflow => { /* See comment above! */ }
        ErrorCode::UnableToDelete => { /* See comment above! */ }
        ErrorCode::AlreadyExecuted => { /* See comment above! */ }
        ErrorCode::InvalidThreshold => { /* See comment above! */ }
    }

    for &error in &all_errors {
        match ProgramError::from(error) {
            ProgramError::Custom(x) if x == error_code => return Some(error),
            _ => continue,
        }
    }

    None
}

#[cfg(test)]
mod test {
    use crate::error::multisig_error_from_u32;
    use solana_program::program_error::ProgramError;

    #[test]
    fn test_multisig_error_from_u32() {
        // We use `assert!matches!` because `ErrorCode` does not implement `Eq`.
        assert!(
            matches!(
                multisig_error_from_u32(301),
                Some(serum_multisig::ErrorCode::NotEnoughSigners),
            ),
            "Note: NotEnoughSigners code is {:?}",
            ProgramError::from(serum_multisig::ErrorCode::NotEnoughSigners),
        );
        assert!(matches!(multisig_error_from_u32(u32::MAX), None));
    }
}

pub fn print_pretty_error_code(error_code: u32) {
    print_key("Error code interpretations:");
    println!("\n");
    match LidoError::from_u32(error_code) {
        Some(err) => println!("    Solido error {} is {:?}", error_code, err),
        None => println!("    Error {} is not a known Solido error.", error_code),
    }
    match multisig_error_from_u32(error_code) {
        Some(multisig_error) => {
            println!(
                "    Multisig error {} is {:?}: {}",
                error_code, multisig_error, multisig_error
            );
        }
        None => {
            println!("    Error {} is not a known Multisig error.", error_code);
        }
    }
    match AnkerError::from_u32(error_code) {
        Some(err) => println!("    Anker error {} is {:?}", error_code, err),
        None => println!("    Error {} is not a known Anker error.", error_code),
    }
}

impl AsPrettyError for ProgramError {
    fn print_pretty(&self) {
        print_red("Program error:");
        match self {
            ProgramError::Custom(error_code) => {
                println!(" Custom error {} (0x{:x})\n", error_code, error_code);
                print_pretty_error_code(*error_code);
            }
            predefined_error => println!(" {:?}", predefined_error),
        }
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

impl AsPrettyError for serde_json::Error {
    fn print_pretty(&self) {
        print_red("Json (de)serialization error:");
        println!(" {:?}", self);
    }
}

impl AsPrettyError for PubkeyError {
    fn print_pretty(&self) {
        print_red("Solana public key error:");
        println!(" {:?}", self);
    }
}

impl AsPrettyError for SignerError {
    fn print_pretty(&self) {
        print_red("Failed to sign transaction: ");
        // `SignerError` does implement display, but the messages are low-quality
        // and not any more helpful than the enum names, so we write custom descriptions
        // here to be a bit more user-friendly.
        match self {
            SignerError::KeypairPubkeyMismatch => {
                println!("Mismatch between keypair and pubkey.");
            }
            SignerError::NotEnoughSigners => {
                println!("Not enough signers.");
                println!(
                    "This is a programming error, please report a bug at \
                    https://github.com/chorusone/solido/issues/new."
                );
            }
            SignerError::TransactionError(err) => {
                println!("Transaction error while signing.");
                err.print_pretty();
            }
            SignerError::Custom(message) => {
                println!("Custom error.");
                print_key("Message:");
                println!(" {}", message)
            }
            SignerError::PresignerError(PresignerError::VerificationFailure) => {
                println!("Pre-signer error.");
                print_key("Message:");
                println!(" {}", PresignerError::VerificationFailure);
            }
            SignerError::Connection(message) => {
                println!("Connection error while signing with remote keypair.");
                print_key("Connection error:");
                println!(" {}", message);
            }
            SignerError::InvalidInput(message) => {
                println!("Invalid input.");
                print_key("Message:");
                println!(" {}", message);
            }
            SignerError::NoDeviceFound => {
                println!("No device found.");
            }
            SignerError::Protocol(message) => {
                println!("Protocol error.");
                print_key("Message:");
                println!(" {}", message);
                // When using the Ledger hardware wallet, if blind signing is
                // disabled in its Solana app, we get "Ledger operation not supported"
                // as message. Try to help the user debug this.
                if message.contains("Ledger") {
                    print_key("Note:");
                    println!(
                        " Is the 'blind signing' setting enabled in the Solana app on the device?"
                    );
                }
            }
            SignerError::UserCancel(message) => {
                println!("Signing cancelled by user.");
                print_key("Message: ");
                println!(" {}", message);
            }
        }
    }
}

impl AsPrettyError for Box<dyn AsPrettyError + 'static> {
    fn print_pretty(&self) {
        (**self).print_pretty()
    }
}

/// Trait for results that we can "unwrap" by pretty-printing and then aborting in case of error.
pub trait Abort {
    type Item;

    /// If the result is an error, pretty-print and abort, otherwise return the `Ok`.
    fn ok_or_abort(self) -> Self::Item;

    /// Print the context message in case of error, then pretty-print the error and abort.
    fn ok_or_abort_with(self, message: &'static str) -> Self::Item;
}

impl<T, E: AsPrettyError> Abort for std::result::Result<T, E> {
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
        match self {
            Ok(result) => result,
            Err(err) => {
                println!("{}", message);
                err.print_pretty();
                std::process::exit(1);
            }
        }
    }
}

impl From<ClientError> for Error {
    fn from(err: ClientError) -> Error {
        Box::new(err)
    }
}

impl From<ProgramError> for Error {
    fn from(err: ProgramError) -> Error {
        Box::new(err)
    }
}

impl From<InstructionError> for Error {
    fn from(err: InstructionError) -> Error {
        // We already have a pretty printer for TransactionError,
        // abuse it a bit by printing instruction errors as transaction errors.
        Box::new(TransactionError::InstructionError(0, err))
    }
}

impl From<TransactionError> for Error {
    fn from(err: TransactionError) -> Error {
        Box::new(err)
    }
}

impl From<PubkeyError> for Error {
    fn from(err: PubkeyError) -> Error {
        Box::new(err)
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Box::new(err)
    }
}

impl From<Box<bincode::ErrorKind>> for Error {
    fn from(err: Box<bincode::ErrorKind>) -> Error {
        Box::new(*err)
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Error {
        Box::new(err)
    }
}
