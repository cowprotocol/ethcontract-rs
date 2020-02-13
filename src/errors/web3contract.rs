//! This module implements adapted `web3` error types so that the errors in
//! the parent module all implement `Sync`. Otherwise, dealing with propagating
//! errors across threads can be tricky.

use ethcontract_common::abi::{Error as AbiError, ErrorKind as AbiErrorKind};
use thiserror::Error;
use web3::error::Error as Web3Error;

/// An addapted `web3::contract::Error` that implements `Sync`.
#[derive(Debug, Error)]
pub enum Web3ContractError {
    /// Invalid output type requested by the caller.
    #[error("invalid output type: {0}")]
    InvalidOutputType(String),
    /// Eth ABI error.
    #[error("ABI error: {0}")]
    Abi(AbiErrorKind),
    /// RPC error.
    #[error("API error: {0}")]
    Api(Web3Error),
}

impl From<web3::contract::Error> for Web3ContractError {
    fn from(err: web3::contract::Error) -> Self {
        match err {
            web3::contract::Error::InvalidOutputType(value) => {
                Web3ContractError::InvalidOutputType(value)
            }
            web3::contract::Error::Abi(AbiError(kind, _)) => Web3ContractError::Abi(kind),
            web3::contract::Error::Api(err) => Web3ContractError::Api(err),
        }
    }
}

impl From<AbiError> for Web3ContractError {
    fn from(err: AbiError) -> Self {
        Web3ContractError::Abi(err.0)
    }
}
