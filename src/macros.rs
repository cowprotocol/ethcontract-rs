//! This module contains macros shared accross the crate.

/// A macro for getting the compiler to assert that a type is always `Unpin`.
///
/// # Examples
///
/// An `Option<T>` is only `Unpin` if `T: Unpin` so the following assertion will
/// fail to compile since `for<T> Option<T>: Unpin` does not hold.
/// ```compile_fail
/// # use ethcontract::assert_unpin;
/// assert_unpin!([T] Option<T>);
/// ```
///
/// However, `Box<T>` is `Unpin` regardless of `T`, so the following assertion
/// will not cause a compilation error.
/// ```no_run
/// # use ethcontract::assert_unpin;
/// assert_unpin!([T] Box<T>);
/// ```
macro_rules! assert_unpin {
    ([$($g:tt)*] $type:ty) => {
        fn __assert_unpin<$($g)*>(value: $type) {
            fn __assert_unpin_inner(_: impl Unpin) {}
            __assert_unpin_inner(value);
        }
    };
}

#[cfg(test)]
#[macro_use]
mod test {
    //! This module contains a collection of utility macros used for unit
    //! testing.

    /// Parse a string address of the form "0x...".
    ///
    /// # Panics
    ///
    /// If the address is invalid.
    macro_rules! addr {
        ($value:expr) => {
            $value[2..]
                .parse::<web3::types::Address>()
                .expect("valid address")
        };
    }

    /// Parse a string uint256 of the form "0x...".
    ///
    /// # Panics
    ///
    /// If the uint is invalid.
    macro_rules! uint {
        ($value:expr) => {
            $value[2..]
                .parse::<web3::types::U256>()
                .expect("valid address")
        };
    }

    /// Parse a string 256-bit hash of the form "0x...".
    ///
    /// # Panics
    ///
    /// If the hash is invalid.
    macro_rules! hash {
        ($value:expr) => {
            $value[2..]
                .parse::<web3::types::H256>()
                .expect("valid hash")
        };
    }

    /// Parse hex encoded string of the form "0x...".
    ///
    /// # Panics
    ///
    /// If the hex string is invalid.
    macro_rules! bytes {
        ($value:expr) => {
            serde_json::from_str::<web3::types::Bytes>(&format!("\"{}\"", $value))
                .expect("valid bytes")
        };
    }

    /// Parse a string 256-bit private key of the form "0x...".
    ///
    /// # Panics
    ///
    /// If the private key is invalid.
    macro_rules! key {
        ($value:expr) => {
            ethsign::SecretKey::from_raw(&hash!($value)[..]).expect("valid key")
        };
    }
}
