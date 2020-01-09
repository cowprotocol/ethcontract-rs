//! Module with common error types.

use ethcontract_common::abi::{Error as AbiError, ErrorKind as AbiErrorKind, Function};
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

    /// Transaction was unable to confirm and is still pending. The contract
    /// address cannot be determined.
    #[error("contract deployment transaction pending: {0}")]
    Pending(H256),
}

impl From<AbiErrorKind> for DeployError {
    fn from(err: AbiErrorKind) -> Self {
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

    /// An error occured while parsing chain ID received from a Web3 call.
    #[error("parse chain ID error: {0}")]
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

    /// A contract transaction failed to confirm within the block timeout limit.
    #[error("transaction confirmation timed-out")]
    ConfirmTimeout,

    /// Transaction failure (e.g. out of gas or revert).
    #[error("transaction failed: {0}")]
    Failure(H256),
}

impl From<Web3Error> for ExecutionError {
    fn from(err: Web3Error) -> Self {
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
    fn from(err: AbiError) -> Self {
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

/// Error that can occur while executing a contract call or transaction.
#[derive(Debug, Error)]
#[error("method '{signature}' failure: {inner}")]
pub struct MethodError {
    /// The signature of the failed method.
    pub signature: String,

    /// The inner execution error that for the method transaction that failed.
    #[source]
    pub inner: ExecutionError,
}

impl MethodError {
    /// Create a new `MethodError` from an ABI function specification and an
    /// inner `ExecutionError`.
    pub fn new<I: Into<ExecutionError>>(function: &Function, inner: I) -> Self {
        MethodError::from_parts(function_signature(function), inner.into())
    }

    /// Create a `MethodError` from its signature and inner `ExecutionError`.
    pub fn from_parts(signature: String, inner: ExecutionError) -> Self {
        MethodError { signature, inner }
    }
}

fn function_signature(function: &Function) -> String {
    format!(
        "{}({})",
        function.name,
        function
            .inputs
            .iter()
            .map(|input| input.kind.to_string())
            .collect::<Vec<_>>()
            .join(","),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonrpc_core::ErrorCode;
    use serde_json::{json, Value};

    fn ganache_rpc_error(data: Value) -> Web3Error {
        Web3Error::Rpc(JsonrpcError {
            code: ErrorCode::from(-32000),
            message: "error".to_owned(),
            data: Some(data),
        })
    }

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

    #[test]
    fn format_function_signature() {
        for (f, expected) in &[
            (r#"{"name":"foo","inputs":[],"outputs":[]}"#, "foo()"),
            (
                r#"{"name":"bar","inputs":[{"name":"a","type":"uint256"},{"name":"b","type":"bool"}],"outputs":[]}"#,
                "bar(uint256,bool)",
            ),
        ] {
            let function: Function = serde_json::from_str(f).expect("invalid function JSON");
            let signature = function_signature(&function);
            assert_eq!(signature, *expected);
        }
    }
}
