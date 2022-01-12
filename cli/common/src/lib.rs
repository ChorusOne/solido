use snapshot::SnapshotError;

pub mod error;
pub mod snapshot;
pub mod validator_info_utils;

pub type Result<T> = std::result::Result<T, SnapshotError>;
