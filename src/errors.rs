//! Module with common error types.

mod ganache;
mod parity;
pub(crate) mod revert;
mod web3contract;

pub use self::web3contract::Web3ContractError;
use ethcontract_common::abi::{Error as AbiError, Event, Function};
use ethcontract_common::abiext::EventExt;
pub use ethcontract_common::errors::*;
use secp256k1::Error as Secp256k1Error;
use std::num::ParseIntError;
use thiserror::Error;
use uint::FromDecStrErr;
use web3::error::Error as Web3Error;
use web3::types::{TransactionReceipt, H256};

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

    /// An error indicating that an attempt was made to build or send a locally
    /// signed transaction to a node without any local accounts.
    #[error("no local accounts")]
    NoLocalAccounts,

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
    #[error("transaction failed: {:?}", .0.transaction_hash)]
    Failure(Box<TransactionReceipt>),

    /// A call returned an unsupported token. This happens when using the
    /// experimental `ABIEncoderV2` option.
    ///
    /// This is intended to be implemented in future version of `ethcontract`.
    #[error("unsupported ABI token")]
    UnsupportedToken,
}

impl From<Web3Error> for ExecutionError {
    fn from(err: Web3Error) -> Self {
        if let Web3Error::Rpc(jsonrpc_err) = &err {
            if let Some(err) = ganache::get_encoded_error(&jsonrpc_err) {
                return err;
            }
            if let Some(err) = parity::get_encoded_error(&jsonrpc_err) {
                return err;
            }
        }

        ExecutionError::Web3(err)
    }
}

impl From<web3::contract::Error> for ExecutionError {
    fn from(err: web3::contract::Error) -> Self {
        ExecutionError::AbiDecode(err.into())
    }
}

impl From<AbiError> for ExecutionError {
    fn from(err: AbiError) -> Self {
        ExecutionError::AbiDecode(err.into())
    }
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
        MethodError::from_parts(function.signature(), inner.into())
    }

    /// Create a `MethodError` from its signature and inner `ExecutionError`.
    pub fn from_parts(signature: String, inner: ExecutionError) -> Self {
        MethodError { signature, inner }
    }
}

/// Error that can occur while streaming contract events.
#[derive(Debug, Error)]
#[error("event '{signature}' failure: {inner}")]
pub struct EventError {
    /// The signature of the failed event.
    pub signature: String,

    /// The inner execution error that for the method transaction that failed.
    #[source]
    pub inner: ExecutionError,
}

impl EventError {
    /// Create a new `EventError` from an ABI function specification and an
    /// inner `ExecutionError`.
    pub fn new<I: Into<ExecutionError>>(event: &Event, inner: I) -> Self {
        EventError::from_parts(event.abi_signature(), inner.into())
    }

    /// Create a `EventError` from its signature and inner `ExecutionError`.
    pub fn from_parts(signature: String, inner: ExecutionError) -> Self {
        EventError { signature, inner }
    }
}

/// An error indicating an invalid private key. Private keys for secp256k1 must
/// be exactly 32 bytes and fall within the range `[1, n)` where `n` is the
/// order of the generator point of the curve.
#[derive(Debug, Error)]
#[error("invalid private key")]
pub struct InvalidPrivateKey;

impl From<Secp256k1Error> for InvalidPrivateKey {
    fn from(err: Secp256k1Error) -> Self {
        match err {
            Secp256k1Error::InvalidSecretKey => {}
            _ => {
                // NOTE: Assert that we never try to make this conversion with
                //   errors not related to `SecretKey`.
                debug_assert!(false, "invalid conversion to InvalidPrivateKey error");
            }
        }
        InvalidPrivateKey
    }
}

/// The error type that is returned when conversion to or from a 256-bit integer
/// fails.
#[derive(Clone, Copy, Debug, Error)]
#[error("output of range integer conversion attempted")]
pub struct TryFromBigIntError;

/// The error type that is returned when parsing a 256-bit signed integer.
#[derive(Clone, Copy, Debug, Error)]
pub enum ParseI256Error {
    /// Error that occurs when an invalid digit is encountered while parsing.
    #[error("invalid digit found in string")]
    InvalidDigit,

    /// Error that occurs when the number is too large or too small (negative)
    /// and does not fit in a 256-bit signed integer.
    #[error("number does not fit in 256-bit integer")]
    IntegerOverflow,
}

impl From<FromDecStrErr> for ParseI256Error {
    fn from(err: FromDecStrErr) -> Self {
        match err {
            FromDecStrErr::InvalidCharacter => ParseI256Error::InvalidDigit,
            FromDecStrErr::InvalidLength => ParseI256Error::IntegerOverflow,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn from_ganache_encoded_error() {
        let web3_err = Web3Error::Rpc(ganache::rpc_error("invalid opcode", None));
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
    fn from_parity_encoded_error() {
        let web3_err = Web3Error::Rpc(parity::rpc_error("Bad instruction fd"));
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
    fn all_errors_are_boxable_errors() {
        fn assert_boxable_error<T: Error + Send + Sync + 'static>() {}

        assert_boxable_error::<DeployError>();
        assert_boxable_error::<ExecutionError>();
        assert_boxable_error::<MethodError>();
        assert_boxable_error::<InvalidPrivateKey>();
    }
}
