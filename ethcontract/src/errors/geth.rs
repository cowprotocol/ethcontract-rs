//! This module implements Geth specific error decoding in order to try and
//! provide more accurate errors from Geth nodes.

use crate::errors::ExecutionError;
use jsonrpc_core::Error as JsonrpcError;

const REVERTED: &str = "execution reverted";
const INVALID_OPCODE: &str = "invalid opcode";

/// Tries to get a more accurate error from a generic Geth JSON RPC error.
/// Returns `None` when a more accurate error cannot be determined.
pub fn get_encoded_error(err: &JsonrpcError) -> Option<ExecutionError> {
    if let Some(str) = err.message.strip_prefix(REVERTED) {
        let reason = str.strip_prefix(": ").map(ToString::to_string);
        Some(ExecutionError::Revert(reason))
    } else if err.message.strip_prefix(INVALID_OPCODE).is_some() {
        Some(ExecutionError::InvalidOpcode)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revert_without_reason() {
        let error = JsonrpcError {
            code: 3.into(),
            message: REVERTED.to_string(),
            data: None,
        };
        let result = get_encoded_error(&error);
        assert!(matches!(result, Some(ExecutionError::Revert(None))));
    }

    #[test]
    fn revert_with_reason() {
        let reason = "SafeMath: subtraction overflow";
        let error = JsonrpcError {
            code: 3.into(),
            message: format!("{}: {}", REVERTED, reason),
            data: None,
        };
        let result = get_encoded_error(&error);
        assert!(matches!(result, Some(ExecutionError::Revert(Some(reason_))) if reason_ == reason));
    }
}
