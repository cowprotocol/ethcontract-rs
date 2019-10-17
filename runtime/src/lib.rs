#![deny(missing_docs)]

//! Runtime crate for `ethcontract` Ethereum contract interaction and code
//! generation. See `ethcontract` crate for more documentation.

pub mod contract;
pub mod sign;
pub mod truffle;

/// A utility module with all the required types for the generated contract type
/// including re-exports from crates of required types, functions and macros.
pub mod ex {
    pub use crate::truffle::Abi;
    pub use lazy_static::lazy_static;
}
