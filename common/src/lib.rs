#![deny(missing_docs, unsafe_code)]

//! Crate for common times shared between the `ethcontract` runtime crate as and
//! the `ethcontract-derive` crate.

pub mod bytecode;
pub mod errors;
pub mod truffle;

pub use crate::bytecode::Bytecode;
pub use crate::truffle::Artifact;
pub use ethabi::{self as abi, Contract as Abi};
pub use web3::types::Address;
