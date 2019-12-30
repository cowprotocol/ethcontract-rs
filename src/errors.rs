//! Module with common error types.

use crate::truffle::abi::{Error as AbiError, ErrorKind as AbiErrorKind};
use ethsign::Error as SignError;
use jsonrpc_core::Error as JsonrpcError;
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

    /// Attempted to deploy a contract when empty bytecode. This can happen when
    /// attempting to deploy a contract that is actually an interface.
    #[error("can not deploy contract with empty bytecode")]
    EmptyBytecode,

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
    Web3(Web3Error),

    /// An error occured while ABI decoding the result of a contract method
    /// call.
    #[error("abi decode error: {0}")]
    AbiDecode(#[from] Web3ContractError),

    /// An error occured while parsing numbers received from Web3 calls.
    #[error("parse error: {0}")]
    Parse(#[from] ParseIntError),

    /// An error occured while signing a transaction offline.
    #[error("offline sign error: {0}")]
    Sign(#[from] SignError),

    /// A contract call reverted.
    #[error("contract call reverted with message: {0:?}")]
    Revert(Option<String>),

    /// A contract call executed an invalid opcode.
    #[error("contract call executed an invalid opcode")]
    InvalidOpcode,
}

impl From<Web3Error> for ExecutionError {
    fn from(err: Web3Error) -> ExecutionError {
        match err {
            Web3Error::Rpc(ref err) if get_error_param(err, "error") == Some("revert") => {
                let reason = get_error_param(err, "reason").map(|reason| reason.to_owned());
                ExecutionError::Revert(reason)
            }
            Web3Error::Rpc(ref err) if get_error_param(err, "error") == Some("invalid opcode") => {
                ExecutionError::InvalidOpcode
            }
            err => ExecutionError::Web3(err),
        }
    }
}

impl From<AbiError> for ExecutionError {
    fn from(err: AbiError) -> ExecutionError {
        ExecutionError::AbiDecode(err.into())
    }
}

/// Gets an error parameters from a JSON RPC error.
///
/// These parameters are the fields inside the transaction object (by tx hash)
/// inside the error data object. Note that we don't need to know the fake tx
/// hash for getting the error params as there should only be one.
fn get_error_param<'a>(err: &'a JsonrpcError, name: &str) -> Option<&'a str> {
    fn is_hash_str(s: &str) -> bool {
        s.len() == 66 && s[2..].parse::<H256>().is_ok()
    }

    err.data
        .as_ref()?
        .as_object()?
        .iter()
        .filter_map(|(k, v)| if is_hash_str(k) { Some(v) } else { None })
        .next()?
        .get(name)?
        .as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonrpc_core::ErrorCode;
    use serde_json::{json, Value};

    #[test]
    fn execution_error_from_ganache_revert_with_message() {
        let web3_err = ganache_rpc_error(json!({
            "0x991fef26454cd1b52e37041295833c24b883e03a2c654fd03bb67e66955e540b": {
               "error": "revert",
               "program_counter": 42,
               "return": "0x08c379a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000076d65737361676500000000000000000000000000000000000000000000000000",
               "reason": "message",
            },
            "stack": "RuntimeError: VM Exception while processing transaction: revert contract reverted as requested ...",
            "name": "RuntimeError",
        }));
        let err = ExecutionError::from(web3_err);

        assert!(
            match err {
                ExecutionError::Revert(Some(ref reason)) if reason == "message" => true,
                _ => false,
            },
            "bad error conversion {:?}",
            err
        );
    }

    #[test]
    fn execution_error_from_ganache_revert() {
        let web3_err = ganache_rpc_error(json!({
            "0x991fef26454cd1b52e37041295833c24b883e03a2c654fd03bb67e66955e540b": {
               "error": "revert",
               "program_counter": 42,
               "return": "0x",
            },
            "stack": "RuntimeError: VM Exception while processing transaction: revert ...",
            "name": "RuntimeError",
        }));
        let err = ExecutionError::from(web3_err);

        assert!(
            match err {
                ExecutionError::Revert(None) => true,
                _ => false,
            },
            "bad error conversion {:?}",
            err
        );
    }

    #[test]
    fn execution_error_from_ganache_invalid_opcode() {
        let web3_err = ganache_rpc_error(json!({
            "0x991fef26454cd1b52e37041295833c24b883e03a2c654fd03bb67e66955e540b": {
               "error": "invalid opcode",
               "program_counter": 42,
               "return": "0x",
            },
            "stack": "RuntimeError: VM Exception while processing transaction: invalid opcode...",
            "name": "RuntimeError",
        }));
        let err = ExecutionError::from(web3_err);

        assert!(
            match err {
                ExecutionError::InvalidOpcode => true,
                _ => false,
            },
            "bad error conversion {:?}",
            err
        );
    }

    fn ganache_rpc_error(data: Value) -> Web3Error {
        Web3Error::Rpc(JsonrpcError {
            code: ErrorCode::from(-32000),
            message: "error".to_owned(),
            data: Some(data),
        })
    }
}
