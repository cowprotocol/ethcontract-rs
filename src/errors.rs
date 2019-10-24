//! Module with common error types.

use ethsign::Error as EthsignError;
use std::num::ParseIntError;
use thiserror::Error;
use web3::contract::Error as Web3ContractError;
use web3::error::Error as Web3Error;

/// Error that can occur while locating a deployed contract.
#[derive(Debug, Error)]
pub enum DeployError {
    /// An error occured while performing a web3 call.
    #[error("web3 error: {0}")]
    Web3(#[from] Web3Error),

    /// No previously deployed contract could be found on the network being used
    /// by the current `web3` provider.
    #[error("could not find deployed contract for network {0}")]
    NotFound(String),
}

/// Error that can occur while executing a contract call or transaction.
#[derive(Debug, Error)]
pub enum ExecutionError {
    /// An error occured while performing a web3 call.
    #[error("web3 error: {0}")]
    Web3(#[from] Web3Error),

    /// An error occured while performing a web3 contract call.
    #[error("web3 contract error: {0}")]
    Web3Contract(#[from] Web3ContractError),

    /// An error occured while parsing numbers received from Web3 calls.
    #[error("parse error: {0}")]
    Parse(#[from] ParseIntError),

    /// An error occured while signing a transaction offline.
    #[error("offline sign error: {0}")]
    Sign(#[from] EthsignError),
}
