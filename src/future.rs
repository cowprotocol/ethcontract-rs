use futures::future::MaybeDone;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Utility type for a future that might be ready. Similar to `MaybeDone` but
/// not fused.
pub struct MaybeReady<F: Future + Unpin>(MaybeDone<F>);

impl<F: Future + Unpin> MaybeReady<F> {
    /// Get a pinned reference to the fused inner `MaybeDone` value.
    fn inner(self: Pin<&mut Self>) -> Pin<&mut MaybeDone<F>> {
        Pin::new(&mut self.get_mut().0)
    }

    /// Create a new `MaybeReady` with an immediate value.
    pub fn ready(value: F::Output) -> MaybeReady<F> {
        MaybeReady(MaybeDone::Done(value))
    }

    /// Create a new `MaybeReady` with a deferred value.
    pub fn future(fut: F) -> MaybeReady<F> {
        MaybeReady(MaybeDone::Future(fut))
    }
}

impl<F: Future + Unpin> Future for MaybeReady<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.as_mut().inner().poll(cx).map(|_| {
            self.inner()
                .take_output()
                .expect("should only be called once")
        })
    }
}
