//! Implementation of string utilities.

use web3::types::Address;

/// Extension trait for in place `String` replacement.
pub trait StringReplaceExt {
    /// Replace all matches of a pattern string with another.
    ///
    /// # Returns
    ///
    /// The number of times the pattern string was found and replaced.
    ///
    /// # Panics
    ///
    /// Panics if the replacement string size does not match the search pattern.
    fn replace_in_place(&mut self, from: &str, to: &str) -> usize;
}

impl StringReplaceExt for String {
    fn replace_in_place(&mut self, from: &str, to: &str) -> usize {
        let len = from.len();
        if to.len() != len {
            panic!("mismatch length of from and to string");
        }

        let mut count = 0;
        let mut last_end = 0;
        while let Some(pos) = self[last_end..].find(from) {
            let start = last_end + pos;
            let end = start + len;

            let section = unsafe { self[start..end].as_bytes_mut() };
            section.copy_from_slice(to.as_bytes());

            count += 1;
            last_end = end;
        }

        count
    }
}

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
    fn replace_in_place() {
        for (value, count, expected) in &[
            ("abcdefg", 0usize, "abcdefg"),
            ("abfoocdefg", 1, "abbarcdefg"),
            ("abfoocdfooefgfoo", 3, "abbarcdbarefgbar"),
        ] {
            let mut value = value.to_string();
            assert_eq!(value.replace_in_place("foo", "bar"), *count);
            assert_eq!(&value, expected);
        }
    }

    #[test]
    fn to_fixed_hex() {
        for (value, expected) in &[
            ("0x0000000000000000000000000000000000000000", "0000000000000000000000000000000000000000"),
            ("0x0102030405060708091020304050607080900001", "0102030405060708091020304050607080900001"),
            ("0x9fac3b52be975567103c4695a2835bba40076da1", "9fac3b52be975567103c4695a2835bba40076da1"),
        ] {
            let value: Address = value[2..].parse().unwrap();
            assert_eq!(&value.to_fixed_hex(), expected);
        }
    }
}
