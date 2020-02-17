//! This module implements extensions to the `ethabi` API.

use crate::hash;
use ethabi::Function;

/// Extension trait for `ethabi::Function`.
pub trait FunctionExt {
    /// Compute the method signature in the standard ABI format.
    fn signature(&self) -> String;

    /// Compute the Keccak256 function selector used by contract ABIs.
    fn selector(&self) -> [u8; 4];
}

impl FunctionExt for Function {
    fn signature(&self) -> String {
        format!(
            "{}({})",
            self.name,
            self.inputs
                .iter()
                .map(|input| input.kind.to_string())
                .collect::<Vec<_>>()
                .join(","),
        )
    }

    fn selector(&self) -> [u8; 4] {
        hash::function_selector(self.signature())
    }
}

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
        ] {
            let function: Function = serde_json::from_str(f).expect("invalid function JSON");
            let signature = function.signature();
            assert_eq!(signature, *expected);
        }
    }
}
