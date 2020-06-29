//! Prelude module with common types used for unit tests.

pub use crate::test::transport::TestTransport;
use futures::future::FutureExt;
pub use serde_json::json;
use std::future::Future;
pub use web3::api::Web3;

/// An extension future to wait for a future.
pub trait FutureTestExt: Future {
    /// Block thread on a future completing.
    fn wait(self) -> Self::Output;
    /// Assert that future is ready immediately and return the output.
    fn immediate(self) -> Self::Output;
}

impl<F: Future + Sized> FutureTestExt for F {
    fn wait(self) -> Self::Output {
        futures::executor::block_on(self)
    }
    fn immediate(self) -> Self::Output {
        self.now_or_never()
            .expect("future did not resolve immediately")
    }
}
