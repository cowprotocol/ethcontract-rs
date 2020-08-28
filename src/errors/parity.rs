//! This module implements Parity specific error decoding in order to try and
//! provide more accurate errors from Parity nodes.

use crate::errors::{revert, ExecutionError};
use jsonrpc_core::Error as JsonrpcError;

/// Revert error discriminant.
const REVERTED: &str = "Reverted 0x";
/// Invalid op-code error discriminant.
const INVALID: &str = "Bad instruction";

/// Tries to get a more accurate error from a generic Parity JSON RPC error.
/// Returns `None` when a more accurate error cannot be determined.
pub fn get_encoded_error(err: &JsonrpcError) -> Option<ExecutionError> {
    let message = get_error_message(err)?;
    if message.starts_with(REVERTED) {
        let hex = &message[REVERTED.len()..];
        if hex.is_empty() {
            return Some(ExecutionError::Revert(None));
        } else {
            let bytes = hex::decode(&hex).ok()?;
            let reason = revert::decode_reason(&bytes)?;
            return Some(ExecutionError::Revert(Some(reason)));
        }
    } else if message.starts_with(INVALID) {
        return Some(ExecutionError::InvalidOpcode);
    }

    None
}

/// Returns the error message from the JSON RPC error data.
fn get_error_message(err: &JsonrpcError) -> Option<&'_ str> {
    err.data.as_ref().and_then(|data| data.as_str())
}

#[cfg(test)]
pub use tests::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::prelude::*;
    use jsonrpc_core::ErrorCode;

    pub fn rpc_error(data: &str) -> JsonrpcError {
        JsonrpcError {
            code: ErrorCode::from(-32015),
            message: "vm execution error".to_owned(),
            data: Some(json!(data)),
        }
    }

    #[test]
    fn execution_error_from_revert_with_message() {
        let jsonrpc_err = rpc_error(&format!(
            "Reverted {}",
            revert::encode_reason_hex("message")
        ));
        let err = get_encoded_error(&jsonrpc_err);

        assert!(
            matches!(
                &err,
                Some(ExecutionError::Revert(Some(reason))) if reason == "message"
            ),
            "bad error conversion {:?}",
            err
        );
    }

    #[test]
    fn execution_error_from_revert() {
        let jsonrpc_err = rpc_error("Reverted 0x");
        let err = get_encoded_error(&jsonrpc_err);

        assert!(
            matches!(err, Some(ExecutionError::Revert(None))),
            "bad error conversion {:?}",
            err
        );
    }

    #[test]
    fn execution_error_from_invalid_opcode() {
        let jsonrpc_err = rpc_error("Bad instruction fd");
        let err = get_encoded_error(&jsonrpc_err);

        assert!(
            matches!(err, Some(ExecutionError::InvalidOpcode)),
            "bad error conversion {:?}",
            err
        );
    }
}
