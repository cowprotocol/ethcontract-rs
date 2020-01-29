//! Implementation of string utilities.

use web3::types::Address;

/// Extension trait for in place `String` replacement.
pub trait StringReplaceExt {
    /// Replace all matches of a pattern string with another.
    ///
    /// # Returns
    ///
    /// Whether a match was found and thus replaced
    /// If from and to are empty returns false and does nothing
    ///
    /// # Panics
    ///
    /// Panics if the replacement string size does not match the search pattern.
    fn replace_all_in_place(&mut self, from: &str, to: &str) -> bool;
}

impl StringReplaceExt for String {
    fn replace_all_in_place(&mut self, from: &str, to: &str) -> bool {
        let len = from.len();
        if to.len() != len {
            panic!("mismatch length of from and to string");
        }

        if len == 0 {
            return false;
        }

        if from == to {
            return self.find(from).is_some();
        }

        let mut found = false;
        while let Some(start) = self.find(from) {
            let end = start + len;

            // NOTE(nlordell): safe since the to string is valid utf-8 and
            //   `str::len()` returns byte length and not character length
            let section = unsafe { self[start..end].as_bytes_mut() };
            section.copy_from_slice(to.as_bytes());

            found = true
        }

        found
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
        for (value, matched, expected) in &[
            ("abcdefg", false, "abcdefg"),
            ("abfoocdefg", true, "abbarcdefg"),
            ("abfoocdfooefgfoo", true, "abbarcdbarefgbar"),
        ] {
            let mut value = (*value).to_string();
            assert_eq!(value.replace_all_in_place("foo", "bar"), *matched);
            assert_eq!(&value, expected);
        }
    }

    #[test]
    fn replace_in_place_empty() {
        assert_eq!("abc".to_string().replace_all_in_place("", ""), false);
    }

    #[test]
    fn replace_in_place_same() {
        assert_eq!("abc".to_string().replace_all_in_place("b", "b"), true);
    }

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
