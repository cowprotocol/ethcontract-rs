use futures::future::{self, Either, Ready};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Utility type for a future that might be ready. Similar to `MaybeDone` but
/// not fused.
pub struct MaybeReady<F: Future>(Either<Ready<F::Output>, F>);

impl<F: Future + Unpin> MaybeReady<F> {
    /// Get a pinned reference to the fused inner `MaybeDone` value.
    fn inner(self: Pin<&mut Self>) -> Pin<&mut Either<Ready<F::Output>, F>> {
        Pin::new(&mut self.get_mut().0)
    }

    /// Create a new `MaybeReady` with an immediate value.
    pub fn ready(value: F::Output) -> MaybeReady<F> {
        MaybeReady(Either::Left(future::ready(value)))
    }

    /// Create a new `MaybeReady` with a deferred value.
    pub fn future(fut: F) -> MaybeReady<F> {
        MaybeReady(Either::Right(fut))
    }
}

impl<F: Future + Unpin> Future for MaybeReady<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.inner().poll(cx)
    }
}
