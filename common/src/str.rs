//! Implementation of string utilities.

use web3::types::Address;

/// Extension trait for converting an `Address` into a hex string implementation.
pub trait AddressHexExt {
    /// Convert an address into a 40 character representation.
    fn to_fixed_hex(&self) -> String;
}

impl AddressHexExt for Address {
    fn to_fixed_hex(&self) -> String {
        format!("{:040x}", self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_fixed_hex() {
        for (value, expected) in &[
            (
                "0x0000000000000000000000000000000000000000",
                "0000000000000000000000000000000000000000",
            ),
            (
                "0x0102030405060708091020304050607080900001",
                "0102030405060708091020304050607080900001",
            ),
            (
                "0x9fac3b52be975567103c4695a2835bba40076da1",
                "9fac3b52be975567103c4695a2835bba40076da1",
            ),
        ] {
            let value: Address = value[2..].parse().unwrap();
            assert_eq!(&value.to_fixed_hex(), expected);
        }
    }
}
