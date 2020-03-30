//! Module implements decoding ABI encoded revert reasons.

use ethcontract_common::abi::{self, ParamType};
use ethcontract_common::hash::{self, H32};
use lazy_static::lazy_static;

lazy_static! {
    /// The ABI function selector for identifying encoded revert reasons.
    static ref ERROR_SELECTOR: H32 = hash::function_selector("Error(string)");
}

/// Decodes an ABI encoded revert reason. Returns `Some(reason)` when the ABI
/// encoded bytes represent a revert reason and `None` otherwise.
///
/// These reasons are prefixed by a 4-byte error followed by an ABI encoded
/// string.
pub fn decode_reason(bytes: &[u8]) -> Option<String> {
    if (bytes.len() + 28) % 32 != 0 || bytes[0..4] != ERROR_SELECTOR[..] {
        // check to make sure that the length is of the form `4 + (n * 32)`
        // bytes and it starts with `keccak256("Error(string)")` which means
        // it is an encoded revert reason from Geth nodes.
        return None;
    }

    let reason = abi::decode(&[ParamType::String], &bytes[4..])
        .ok()?
        .pop()
        .expect("decoded single parameter will yield single token")
        .to_string()
        .expect("decoded string parameter will always be a string");

    Some(reason)
}

#[cfg(test)]
pub use tests::*;

#[cfg(test)]
mod tests {
    use super::*;
    use ethcontract_common::abi::{Function, Param, Token};

    pub fn encode_reason(reason: &str) -> Vec<u8> {
        let revert = Function {
            name: "Error".into(),
            inputs: vec![Param {
                name: "".into(),
                kind: ParamType::String,
            }],
            outputs: Vec::new(),
            constant: true,
        };
        revert
            .encode_input(&[Token::String(reason.into())])
            .expect("error encoding revert reason")
    }

    pub fn encode_reason_hex(reason: &str) -> String {
        let encoded = encode_reason(reason);
        format!("0x{}", hex::encode(encoded))
    }

    #[test]
    fn decode_revert_reason() {
        let reason = "ethcontract rocks!";
        let encoded = encode_reason(reason);

        assert_eq!(decode_reason(&encoded).as_deref(), Some(reason));
    }
}
