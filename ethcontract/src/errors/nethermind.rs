//! This module implements Nethermind specific error decoding in order to try and
//! provide more accurate errors from Nethermind nodes.

use crate::errors::{revert, ExecutionError};
use jsonrpc_core::Error as JsonrpcError;

/// Revert error discriminant.
const REVERTED: &str = "Reverted 0x";
/// Invalid op-code error discriminant.
const INVALID: &str = "Bad instruction";
/// Error messages for VM execution errors.
const MESSAGES: &[&str] = &["VM execution error", "VM execution error."];

/// Tries to get a more accurate error from a generic Nethermind JSON RPC error.
/// Returns `None` when a more accurate error cannot be determined.
pub fn get_encoded_error(err: &JsonrpcError) -> Option<ExecutionError> {
    let message = get_error_message(err);
    if let Some(hex) = message.strip_prefix(REVERTED) {
        if hex.is_empty() {
            return Some(ExecutionError::Revert(None));
        } else {
            match hex::decode(hex)
                .ok()
                .and_then(|bytes| revert::decode_reason(&bytes))
            {
                Some(reason) => return Some(ExecutionError::Revert(Some(reason))),
                None => return Some(ExecutionError::Revert(None)),
            }
        }
    } else if message.starts_with(INVALID) {
        return Some(ExecutionError::InvalidOpcode);
    }

    if MESSAGES.contains(&&*err.message) {
        return Some(ExecutionError::Revert(None));
    }

    None
}

/// Returns the error message from the JSON RPC error data.
fn get_error_message(err: &JsonrpcError) -> &str {
    err.data
        .as_ref()
        .and_then(|data| data.as_str())
        .unwrap_or(&err.message)
}

#[cfg(test)]
pub use tests::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::prelude::*;
    use jsonrpc_core::ErrorCode;

    pub fn rpc_errors(data: &str) -> Vec<JsonrpcError> {
        // Nethermind has two flavours of revert errors:
        // ```
        // {"jsonrpc":"2.0","error":{"code":-32015,"message":"VM execution error.","data":"Reverted 0x..."},"id":0}
        // {"jsonrpc":"2.0","error":{"code":-32015,"message":"Reverted 0x..."},"id":1}
        // ```
        vec![
            JsonrpcError {
                code: ErrorCode::from(-32015),
                message: "VM execution error".to_owned(),
                data: Some(json!(data)),
            },
            JsonrpcError {
                code: ErrorCode::from(-32015),
                message: data.to_owned(),
                data: None,
            },
        ]
    }

    #[test]
    fn execution_error_from_revert_with_message() {
        for jsonrpc_err in rpc_errors(&format!(
            "Reverted {}",
            revert::encode_reason_hex("message")
        )) {
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
    }

    #[test]
    fn execution_error_from_revert() {
        for jsonrpc_err in rpc_errors("Reverted 0x") {
            let err = get_encoded_error(&jsonrpc_err);

            assert!(
                matches!(err, Some(ExecutionError::Revert(None))),
                "bad error conversion {:?}",
                err
            );
        }
    }

    #[test]
    fn execution_error_from_revert_failed_decode() {
        for jsonrpc_err in rpc_errors("Reverted 0x01020304") {
            let err = get_encoded_error(&jsonrpc_err);

            assert!(
                matches!(err, Some(ExecutionError::Revert(None))),
                "bad error conversion {:?}",
                err
            );
        }
    }

    #[test]
    fn execution_error_from_invalid_opcode() {
        for jsonrpc_err in rpc_errors("Bad instruction fd") {
            let err = get_encoded_error(&jsonrpc_err);

            assert!(
                matches!(err, Some(ExecutionError::InvalidOpcode)),
                "bad error conversion {:?}",
                err
            );
        }
    }

    #[test]
    fn execution_error_from_message() {
        let jsonrpc_err = JsonrpcError {
            code: ErrorCode::from(-32015),
            message: "VM execution error.".to_owned(),
            data: Some(json!("revert")),
        };
        let err = get_encoded_error(&jsonrpc_err);

        assert!(
            matches!(err, Some(ExecutionError::Revert(None))),
            "bad error conversion {:?}",
            err
        );
    }
}
