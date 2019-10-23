use futures::compat::Compat01As03;
use std::ops::Deref;
use web3::api::Web3;
use web3::confirm::SendTransactionWithConfirmation;
use web3::contract::QueryResult;
use web3::helpers::CallFuture;
use web3::Transport;

/// Helper type for wrapping `Web3` as `Unpin`.
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

// It is safe to mark this type as `Unpin` since `Web3<T>` *should be* `Unpin`
// even if T is not.
// TODO(nlordell): verify this is safe
impl<T: Transport> Unpin for Web3Unpin<T> {}

/// Type alias for Compat01As03<CallFuture<...>> since it is used a lot.
pub type CompatCallFuture<T, R> = Compat01As03<CallFuture<R, <T as Transport>::Out>>;

/// Type alias for Compat01As03<QueryResult<...>>.
pub type CompatQueryResult<T, R> = Compat01As03<QueryResult<R, <T as Transport>::Out>>;

/// Type alias for Compat01As03<SendTransactionWithConfirmation<...>>.
pub type CompatSendTxWithConfirmation<T> = Compat01As03<SendTransactionWithConfirmation<T>>;
