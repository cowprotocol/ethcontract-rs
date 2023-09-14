//! This module implements Hardhat specific error decoding in order to try and
//! provide more accurate errors from Hardhat nodes.
//!
//! Error messages can be found here:
//! <https://github.com/NomicFoundation/hardhat/blob/d3278835257841dd62d619c00f53f908ffb5f743/packages/hardhat-core/src/internal/hardhat-network/stack-traces/solidity-errors.ts#L217>

use crate::errors::ExecutionError;
use jsonrpc_core::Error as JsonrpcError;

/// Tries to get a more accurate error from a generic Hardhat JSON RPC error.
/// Returns `None` when a more accurate error cannot be determined.
pub fn get_encoded_error(err: &JsonrpcError) -> Option<ExecutionError> {
    if err.message == "VM Exception while processing transaction: invalid opcode" {
        return Some(ExecutionError::InvalidOpcode);
    }

    if let Some(reason) = err
        .message
        .strip_prefix("VM Exception while processing transaction: reverted with reason string '")
        .and_then(|rest| rest.strip_suffix('\''))
    {
        return Some(ExecutionError::Revert(Some(reason.to_owned())));
    }

    for needle in ["VM Exception", "Transaction reverted"] {
        if err.message.contains(needle) {
            return Some(ExecutionError::Revert(None));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::prelude::*;
    use jsonrpc_core::ErrorCode;

    #[test]
    fn execution_error_from_revert_with_message() {
        let jsonrpc_err = JsonrpcError {
            code: ErrorCode::InternalError,
            message: "Error: VM Exception while processing transaction: \
                      reverted with reason string 'GS020'"
                .to_owned(),
            data: Some(json!({
                "data": "0x08c379a0\
                           0000000000000000000000000000000000000000000000000000000000000020\
                           0000000000000000000000000000000000000000000000000000000000000005\
                           4753303230000000000000000000000000000000000000000000000000000000",
                "message": "Error: VM Exception while processing transaction: \
                            reverted with reason string 'GS020'"
            })),
        };
        let err = get_encoded_error(&jsonrpc_err);

        assert!(
            matches!(
                &err,
                Some(ExecutionError::Revert(Some(reason))) if reason == "message"
            ),
            "bad error conversion {err:?}",
        );
    }
}
