#![deny(missing_docs)]

//! TODO(nlordell): documentation with examples.

pub mod contract;
pub mod errors;
mod future;
pub mod sign;
pub mod transaction;
pub mod transport;

pub use ethcontract_common::*;
pub use ethcontract_derive::contract;
pub use serde_json as json;
pub use web3;

use crate::contract::Instance;
use crate::transport::DynTransport;

/// Type alias for a contract `Instance` with an underyling `DynTransport`.
pub type DynInstance = Instance<DynTransport>;

#[doc(hidden)]
pub mod foreign {
    //! Foreign types that we re-export to be used internally by the procedural
    //! macro but do not appear on public interfaces.

    pub use lazy_static::lazy_static;
}

#[allow(missing_docs)]
pub mod example {
    //! An example of a derived contract for documentation purposes in roder to
    //! illustrate what the generated API. This module should not be used and is
    //! should only be included when generating documentation.

    use crate as ethcontract;

    ethcontract::contract!("examples/WETH9.json");

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn example_loads_artifact() {
            // just make sure this does not panic!
            Weth9::artifact();
        }
    }
}
