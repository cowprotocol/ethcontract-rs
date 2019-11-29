use futures::compat::Compat01As03;
use futures::future::{self, Either, Ready};
use std::future::Future;
use std::ops::Deref;
use std::pin::Pin;
use std::task::{Context, Poll};
use web3::api::Web3;
use web3::confirm::SendTransactionWithConfirmation;
use web3::contract::QueryResult;
use web3::helpers::CallFuture;
use web3::Transport;

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

/// Helper type for wrapping `Web3` as `Unpin`.
#[derive(Clone, Debug)]
pub struct Web3Unpin<T: Transport>(Web3<T>);

impl<T: Transport> Into<Web3<T>> for Web3Unpin<T> {
    fn into(self) -> Web3<T> {
        self.0
    }
}

impl<T: Transport> From<Web3<T>> for Web3Unpin<T> {
    fn from(web3: Web3<T>) -> Self {
        Web3Unpin(web3)
    }
}

impl<T: Transport> Deref for Web3Unpin<T> {
    type Target = Web3<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// NOTE(nlordell): It is safe to mark this type as `Unpin` since `Web3<T>`
//   *should be* `Unpin` even if T is not.
// TODO(nlordell): verify this is safe
impl<T: Transport> Unpin for Web3Unpin<T> {}

/// Type alias for Compat01As03<CallFuture<...>> since it is used a lot.
pub type CompatCallFuture<T, R> = Compat01As03<CallFuture<R, <T as Transport>::Out>>;

/// Type alias for Compat01As03<QueryResult<...>>.
pub type CompatQueryResult<T, R> = Compat01As03<QueryResult<R, <T as Transport>::Out>>;

/// Type alias for Compat01As03<SendTransactionWithConfirmation<...>>.
pub type CompatSendTxWithConfirmation<T> = Compat01As03<SendTransactionWithConfirmation<T>>;
