//! This module implements Ganache specific error decoding in order to try and
//! provide more accurate errors from Ganache nodes.

use crate::errors::ExecutionError;
use jsonrpc_core::Error as JsonrpcError;
use web3::types::H256;

/// Tries to get a more accurate error from a generic Ganache JSON RPC error.
/// Returns `None` when a more accurate error cannot be determined.
pub fn get_encoded_error(err: &JsonrpcError) -> Option<ExecutionError> {
    match get_error_param(err, "error") {
        Some("revert") => {
            let reason = get_error_param(err, "reason").map(|reason| reason.to_owned());
            Some(ExecutionError::Revert(reason))
        }
        Some("invalid opcode") => Some(ExecutionError::InvalidOpcode),
        _ => None,
    }
}

/// Gets an error parameters from a Ganache JSON RPC error.
///
/// these parameters are the fields inside the transaction object (by tx hash)
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
        .find_map(|(k, v)| if is_hash_str(k) { Some(v) } else { None })?
        .get(name)?
        .as_str()
}

#[cfg(test)]
pub use tests::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::revert;
    use crate::test::prelude::*;
    use jsonrpc_core::ErrorCode;
    use std::borrow::Cow;

    pub fn rpc_error(error: &str, reason: Option<&str>) -> JsonrpcError {
        let return_data: Cow<str> = if let Some(reason) = reason {
            revert::encode_reason_hex(reason).into()
        } else {
            "0x".into()
        };

        JsonrpcError {
            code: ErrorCode::from(-32000),
            message: "error".to_owned(),
            data: Some(json!({
                "0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f": {
                   "error": error,
                   "program_counter": 42,
                   "return": return_data,
                   "reason": reason,
                },
                "stack": "RuntimeError: VM Exception while processing transaction ...",
                "name": "RuntimeError",
            })),
        }
    }

    #[test]
    fn execution_error_from_revert_with_message() {
        let jsonrpc_err = rpc_error("revert", Some("message"));
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
        let jsonrpc_err = rpc_error("revert", None);
        let err = get_encoded_error(&jsonrpc_err);

        assert!(
            matches!(err, Some(ExecutionError::Revert(None))),
            "bad error conversion {:?}",
            err
        );
    }

    #[test]
    fn execution_error_from_invalid_opcode() {
        let jsonrpc_err = rpc_error("invalid opcode", None);
        let err = get_encoded_error(&jsonrpc_err);

        assert!(
            matches!(err, Some(ExecutionError::InvalidOpcode)),
            "bad error conversion {:?}",
            err
        );
    }
}
