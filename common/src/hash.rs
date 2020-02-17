//! Keccak256 hash utilities.

use tiny_keccak::{Hasher, Keccak};

/// Perform a Keccak256 hash of data and return its 32-byte result.
pub fn keccak256<B>(data: B) -> [u8; 32]
where
    B: AsRef<[u8]>,
{
    let mut output = [0u8; 32];
    let mut hasher = Keccak::v256();
    hasher.update(data.as_ref());
    hasher.finalize(&mut output);
    output
}

/// Calculate the function selector as per the contract ABI specification. This
/// is definied as the first 4 bytes of the Keccak256 hash of the function
/// signature.
pub fn function_selector<S>(signature: S) -> [u8; 4]
where
    S: AsRef<str>,
{
    let hash = keccak256(signature.as_ref());
    let mut selector = [0u8; 4];
    selector.copy_from_slice(&hash[0..4]);
    selector
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_keccak_hash() {
        // test vector retrieved from
        // https://web3js.readthedocs.io/en/v1.2.4/web3-utils.html#sha3
        assert_eq!(
            &keccak256([0xea]),
            b"\x2f\x20\x67\x74\x59\x12\x06\x77\x48\x4f\x71\x04\xc7\x6d\xeb\x68\
              \x46\xa2\xc0\x71\xf9\xb3\x15\x2c\x10\x3b\xb1\x2c\xd5\x4d\x1a\x4a"
        );
    }

    #[test]
    fn simple_function_signature() {
        // test vector retrieved from
        // https://web3js.readthedocs.io/en/v1.2.4/web3-eth-abi.html#encodefunctionsignature
        assert_eq!(
            function_selector("myMethod(uint256,string)"),
            [0x24, 0xee, 0x00, 0x97],
        );
    }

    #[test]
    fn revert_function_signature() {
        assert_eq!(function_selector("Error(string)"), [0x08, 0xc3, 0x79, 0xa0]);
    }
}
