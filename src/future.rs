#![allow(dead_code)]

use futures::compat::Compat01As03;
use futures::future::{self, Either, Ready};
use pin_project::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use web3::helpers::CallFuture;
use web3::Transport;

/// Utility type for a future that might be ready. Similar to `MaybeDone` but
/// not fused.
#[pin_project]
#[derive(Debug)]
pub struct MaybeReady<F: Future>(#[pin] Either<Ready<F::Output>, F>);

impl<F: Future> MaybeReady<F> {
    /// Create a new `MaybeReady` with an immediate value.
    pub fn ready(value: F::Output) -> Self {
        MaybeReady(Either::Left(future::ready(value)))
    }

    /// Create a new `MaybeReady` with a deferred value.
    pub fn future(fut: F) -> Self {
        MaybeReady(Either::Right(fut))
    }
}

impl<F: Future> Future for MaybeReady<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.project().0.poll(cx)
    }
}

/// Type alias for Compat01As03<CallFuture<...>> since it is used a lot.
pub type CompatCallFuture<T, R> = Compat01As03<CallFuture<R, <T as Transport>::Out>>;
