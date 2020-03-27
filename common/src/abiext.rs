//! This module implements extensions to the `ethabi` API.

use crate::errors::ParseParamTypeError;
use crate::hash::{self, H32};
use ethabi::{Event, Function, ParamType};
use serde_json::json;

/// Extension trait for `ethabi::Function`.
pub trait FunctionExt {
    /// Compute the method signature in the standard ABI format. This does not
    /// include the output types.
    fn abi_signature(&self) -> String;

    /// Compute the Keccak256 function selector used by contract ABIs.
    fn selector(&self) -> H32;
}

impl FunctionExt for Function {
    fn abi_signature(&self) -> String {
        let mut full_signature = self.signature();
        if let Some(colon) = full_signature.find(':') {
            full_signature.truncate(colon);
        }

        full_signature
    }

    fn selector(&self) -> H32 {
        hash::function_selector(self.abi_signature())
    }
}

/// Extension trait for `ethabi::Event`.
pub trait EventExt {
    /// Compute the event signature in human-readable format. The `keccak256`
    /// hash of this value is the actual event signature that is used as topic0
    /// in the transaction logs.
    fn abi_signature(&self) -> String;
}

impl EventExt for Event {
    fn abi_signature(&self) -> String {
        format!(
            "{}({}){}",
            self.name,
            self.inputs
                .iter()
                .map(|input| input.kind.to_string())
                .collect::<Vec<_>>()
                .join(","),
            if self.anonymous { " anonymous" } else { "" },
        )
    }
}

/// An extension trait for Solidity parameter types.
pub trait ParamTypeExt {
    /// Parses a parameter type from a string value.
    fn from_str(s: &str) -> Result<ParamType, ParseParamTypeError> {
        serde_json::from_value(json!(s)).map_err(|_| ParseParamTypeError(s.into()))
    }
}

impl ParamTypeExt for ParamType {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_function_signature() {
        for (f, expected) in &[
            (r#"{"name":"foo","inputs":[],"outputs":[]}"#, "foo()"),
            (
                r#"{"name":"bar","inputs":[{"name":"a","type":"uint256"},{"name":"b","type":"bool"}],"outputs":[]}"#,
                "bar(uint256,bool)",
            ),
            (
                r#"{"name":"baz","inputs":[{"name":"a","type":"uint256"}],"outputs":[{"name":"b","type":"bool"}]}"#,
                "baz(uint256)",
            ),
            (
                r#"{"name":"bax","inputs":[],"outputs":[{"name":"a","type":"uint256"},{"name":"b","type":"bool"}]}"#,
                "bax()",
            ),
        ] {
            let function: Function = serde_json::from_str(f).expect("invalid function JSON");
            let signature = function.abi_signature();
            assert_eq!(signature, *expected);
        }
    }

    #[test]
    fn format_event_signature() {
        for (e, expected) in &[
            (r#"{"name":"foo","inputs":[],"anonymous":false}"#, "foo()"),
            (
                r#"{"name":"bar","inputs":[{"name":"a","type":"uint256"},{"name":"b","type":"bool"}],"anonymous":false}"#,
                "bar(uint256,bool)",
            ),
            (
                r#"{"name":"baz","inputs":[{"name":"a","type":"uint256"}],"anonymous":true}"#,
                "baz(uint256) anonymous",
            ),
            (
                r#"{"name":"bax","inputs":[],"anonymous":true}"#,
                "bax() anonymous",
            ),
        ] {
            let event: Event = serde_json::from_str(e).expect("invalid event JSON");
            let signature = event.abi_signature();
            assert_eq!(signature, *expected);
        }
    }
}
