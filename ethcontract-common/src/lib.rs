#![deny(missing_docs, unsafe_code)]

//! Crate for common times shared between the `ethcontract` runtime crate as and
//! the `ethcontract-derive` crate.

pub mod abiext;
pub mod bytecode;
pub mod contract;
pub mod errors;
pub mod hash;

pub use crate::abiext::FunctionExt;
pub use crate::bytecode::Bytecode;
pub use crate::contract::Contract;
pub use ethabi::{self as abi, Contract as Abi};
use serde::Deserialize;
pub use web3::types::Address;
pub use web3::types::H256 as TransactionHash;

/// Information about when a contract instance was deployed
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum DeploymentInformation {
    /// The block at which the contract was deployed
    BlockNumber(u64),
    /// The transaction hash at which the contract was deployed
    TransactionHash(TransactionHash),
}

impl From<u64> for DeploymentInformation {
    fn from(block: u64) -> Self {
        Self::BlockNumber(block)
    }
}

impl From<TransactionHash> for DeploymentInformation {
    fn from(hash: TransactionHash) -> Self {
        Self::TransactionHash(hash)
    }
}
