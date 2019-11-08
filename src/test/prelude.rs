//! Prelude module with common types used for unit tests.

pub use crate::test::macros::*;
pub use crate::test::transport::TestTransport;
pub use serde_json::json;
use std::future::Future;
pub use web3::api::Web3;

/// Temporary solution to work around the issue that async tests are not stable
/// and the extra boiler plate of setting up an executor is inconvenient.
/// TODO(nlordell): remove once async tests stablalize
pub trait FutureWaitExt: Future {
    /// Block thread on a future completing.
    fn wait(self) -> Self::Output;
}

impl<F: Future + Sized> FutureWaitExt for F {
    fn wait(self) -> Self::Output {
        futures::executor::block_on(self)
    }
}
