//! Prelude module with common types used for unit tests.

pub use crate::test::macros::*;
pub use crate::test::transport::TestTransport;
use futures::future::FutureExt;
pub use serde_json::json;
use std::future::Future;
pub use web3::api::Web3;

/// An extension future to assert that a future resolves immediately.
pub trait FutureImmediateExt: Future {
    /// Block thread on a future completing.
    fn immediate(self) -> Self::Output;
}

impl<F: Future + Sized> FutureImmediateExt for F {
    fn immediate(self) -> Self::Output {
        self.now_or_never()
            .expect("future did not resolve immediately")
    }
}
