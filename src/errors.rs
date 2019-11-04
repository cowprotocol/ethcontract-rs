//! Module with common error types.

use ethabi::{Error as AbiError, ErrorKind as AbiErrorKind};
use ethsign::Error as SignError;
use std::num::ParseIntError;
use thiserror::Error;
use web3::contract::Error as Web3ContractError;
use web3::error::Error as Web3Error;
use web3::types::H256;

pub use ethcontract_common::errors::*;

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

    /// Error linking a contract with a deployed library.
    #[error("could not link library {0}")]
    Link(#[from] LinkError),

    /// An error occured encoding deployment parameters with the contract ABI.
    #[error("error ABI ecoding deployment parameters: {0}")]
    Abi(#[from] AbiError),

    /// Error executing contract deployment transaction.
    #[error("error executing contract deployment transaction: {0}")]
    Tx(#[from] ExecutionError),

    /// Transaction failure (e.g. out of gas).
    #[error("contract deployment transaction failed: {0}")]
    Failure(H256),
}

impl From<AbiErrorKind> for DeployError {
    fn from(err: AbiErrorKind) -> DeployError {
        DeployError::Abi(err.into())
    }
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
    Sign(#[from] SignError),
}
