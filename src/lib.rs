#![deny(missing_docs, unsafe_code)]

//! Generate bindings for Ethereum smart contracts. Internally, the generated
//! types use `web3` crate to interact with the Ethereum network and uses a
//! custom [`Instance`](ethcontract::contract::Instance) runtime that can be
//! used directly without code generation.
//!
//! This crate is using `std::future::Future` for futures by wrapping `web3`
//! `futures` 0.1 with `futures` 0.3 compatibility layer. This means that this
//! crate is ready for `async`/`await`!
//!
//! Here is an example of interacing with a smart contract `MyContract`. The
//! builder pattern is used for configuring transactions.
//!
//! ```text
//! pragma solidity ^0.5.0;
//!
//! contract MyContract {
//!   function my_view_function(uint64 some_val) public view returns (string) {
//!     // ...
//!   }
//!
//!   function my_function(bool some_bool, string some_str) public returns (uint256) {
//!     // ...
//!   }
//!
//!   function my_other_function() public {
//!     // ...
//!   }
//! }
//! ```
//!
//! Once this contract is built and deployed with truffle the following example
//! demonstrates how to interact with it from Rust.
//!
//! ```ignore
//! use ethcontract::transaction::Account;
//! use std::time::Duration;
//! use web3::api::Web3;
//! use web3::types::*;
//!
//! // this proc macro generates a `MyContract` type with type-safe bindings to
//! // contract functions
//! ethcontract::contract!("path/to/MyContract.json");
//!
//! // create a web3 instance as usual
//! let transport = ...;
//! let web3 = Web3::new(transport);
//!
//! // now create an instance of an interface to the contract
//! let instance = MyContract::deployed(web3).await?;
//!
//! let addr: Address = "0x000102030405060708090a0b0c0d0e0f10111213".parse()?;
//! let some_uint: U256 = U256::from_dec_str("1000000000000000000")?;
//! let some_bool = true;
//! let some_val = u64;
//!
//! // call instance view functions with type-safe bindings! will only compile
//! // if contract function accepts a single `u64` value parameter and returns
//! // a concrete type based on the contract function's return type
//! let value = instance
//!     .my_view_function(some_val)
//!     .from(addr)
//!     .execute()
//!     .await?;
//!
//! // contract functions also have type-safe bindings and return the tx hash
//! // of the submitted transaction; allows for multiple ways of signing txs
//! let tx = instance
//!     .my_function(some_bool, value)
//!     .from(Account::Locked(addr, "password".into(), None))
//!     .value(some_uint)
//!     .gas_price(1_000_000.into())
//!     .execute()
//!     .await?;
//!
//! // wait for confirmations when needed
//! let receipt = instance
//!     .my_important_function()
//!     .poll_interval(Duration::from_secs(5))
//!     .confirmations(2)
//!     .execute_confirm()
//!     .await?;
//! ```
//!
//! See [`contract!`](ethcontract::contract) proc macro documentation for more
//! information on usage and parameters as well on how to use contract ABI
//! directly from Etherscan.

#[cfg(test)]
#[allow(missing_docs)]
#[macro_use]
#[path = "test/macros.rs"]
mod test_macros;

pub mod contract;
mod conv;
pub mod errors;
mod future;
mod int;
pub mod log;
pub mod secret;
pub mod sign;
pub mod transaction;
pub mod transport;

pub use crate::contract::Instance;
pub use crate::prelude::*;
pub use ethcontract_common as common;
pub use ethcontract_common::truffle::Artifact;
#[cfg(feature = "derive")]
pub use ethcontract_derive::contract;
pub use jsonrpc_core as jsonrpc;
pub use serde_json as json;
pub use web3;

pub mod prelude {
    //! A prelude module for importing commonly used types when interacting with
    //! generated contracts.

    pub use crate::contract::{Event, EventData, EventMetadata, RawLog, Topic, Void};
    pub use crate::int::I256;
    pub use crate::secret::{Password, PrivateKey};
    pub use crate::transaction::{Account, GasPrice};
    pub use web3::api::Web3;
    pub use web3::transports::Http;
    pub use web3::types::{Address, BlockNumber, TransactionCondition, H160, H256, U256};
}

pub mod dyns {
    //! Type aliases to various runtime types that use an underlying
    //! `DynTransport`. These types are used extensively throughout the
    //! generated code.

    use crate::contract::{
        AllEventsBuilder, DeployBuilder, EventBuilder, Instance, MethodBuilder, ViewMethodBuilder,
    };
    pub use crate::transport::DynTransport;
    use web3::api::Web3;

    /// Type alias for a `Web3` with an underlying `DynTransport`.
    pub type DynWeb3 = Web3<DynTransport>;

    /// Type alias for an `Instance` with an underlying `DynTransport`.
    pub type DynInstance = Instance<DynTransport>;

    /// Type alias for a `DeployBuilder` with an underlying `DynTransport`.
    pub type DynDeployBuilder<D> = DeployBuilder<DynTransport, D>;

    /// Type alias for a `MethodBuilder` with an underlying `DynTransport`.
    pub type DynMethodBuilder<R> = MethodBuilder<DynTransport, R>;

    /// Type alias for a `ViewMethodBuilder` with an underlying `DynTransport`.
    pub type DynViewMethodBuilder<R> = ViewMethodBuilder<DynTransport, R>;

    /// Type alias for a `EventBuilder` with an underlying `DynTransport`.
    pub type DynEventBuilder<E> = EventBuilder<DynTransport, E>;

    /// Type alias for a `LogStream` with an underlying `DynTransport`.
    pub type DynAllEventsBuilder<E> = AllEventsBuilder<DynTransport, E>;
}

#[doc(hidden)]
pub mod private {
    //! Private definitions that are needed by the generated contract code or
    //! but do not appear in public interfaces. No documentation is generated
    //! for these definitions.

    pub use lazy_static::lazy_static;
}

#[cfg(test)]
#[allow(missing_docs)]
mod test {
    pub mod prelude;
    pub mod transport;
}

#[cfg(feature = "samples")]
#[allow(missing_docs)]
pub mod samples {
    //! Samples of derived contracts for documentation purposes in roder to
    //! illustrate what the generated API. This module should not be used and is
    //! should only be included when generating documentation.

    crate::contract!(
        "examples/truffle/build/contracts/DocumentedContract.json",
        crate = crate
    );
    crate::contract!(
        "examples/truffle/build/contracts/SimpleLibrary.json",
        crate = crate
    );
    crate::contract!(
        "examples/truffle/build/contracts/LinkedContract.json",
        crate = crate
    );
    crate::contract!(
        "examples/truffle/build/contracts/IERC20.json",
        crate = crate
    );
}
