//! This module implements adapted `web3` error types so that the errors in
//! the parent module all implement `Sync`. Otherwise, dealing with propagating
//! errors across threads can be tricky.

use crate::abicompat::AbiCompat;
use ethcontract_common::abi::Error as AbiError;
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
    Abi(#[from] AbiError),
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
            web3::contract::Error::Abi(err) => Web3ContractError::Abi(err.compat()),
            web3::contract::Error::Api(err) => Web3ContractError::Api(err),
        }
    }
}
