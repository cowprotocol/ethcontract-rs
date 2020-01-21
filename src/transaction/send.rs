//! Implementation of a future for sending a transaction with optional
//! confirmation.

use crate::errors::ExecutionError;
use crate::future::CompatCallFuture;
use crate::transaction::build::BuildFuture;
use crate::transaction::confirm::ConfirmFuture;
use crate::transaction::{ResolveCondition, Transaction, TransactionBuilder, TransactionResult};
use futures::compat::Future01CompatExt;
use futures::ready;
use pin_project::{pin_project, project};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use web3::api::Web3;
use web3::types::{H256, U64};
use web3::Transport;

/// Future for optionally signing and then sending a transaction.
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[pin_project]
pub struct SendFuture<T: Transport> {
    web3: Web3<T>,
    /// The confirmation options to use for the transaction once it has been
    /// sent. Stored as an option as we require transfer of ownership.
    resolve: Option<ResolveCondition>,
    /// Internal execution state.
    #[pin]
    state: SendState<T>,
}

/// The state of the send future.
#[pin_project]
enum SendState<T: Transport> {
    /// The transaction is being built into a request or a signed raw
    /// transaction.
    Building(#[pin] BuildFuture<T>),
    /// The transaction is being sent to the node.
    Sending(#[pin] CompatCallFuture<T, H256>),
    /// The transaction is being confirmed.
    Confirming(#[pin] ConfirmFuture<T>),
}

impl<T: Transport> SendFuture<T> {
    /// Creates a new future from a `TransactionBuilder`
    pub fn from_builder(mut builder: TransactionBuilder<T>) -> Self {
        let web3 = builder.web3.clone();
        let resolve = Some(builder.resolve.take().unwrap_or_default());
        let state = SendState::Building(BuildFuture::from_builder(builder));

        SendFuture {
            web3,
            resolve,
            state,
        }
    }
}

impl<T: Transport> Future for SendFuture<T> {
    type Output = Result<TransactionResult, ExecutionError>;

    #[project]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        loop {
            #[project]
            let SendFuture {
                web3,
                resolve,
                state,
            } = self.as_mut().project();

            #[project]
            let next_state = match state.project() {
                SendState::Building(build) => {
                    let tx = match ready!(build.poll(cx)) {
                        Ok(tx) => tx,
                        Err(err) => return Poll::Ready(Err(err)),
                    };

                    let eth = web3.eth();
                    let send = match tx {
                        Transaction::Request(tx) => eth.send_transaction(tx).compat(),
                        Transaction::Raw(tx) => eth.send_raw_transaction(tx).compat(),
                    };

                    SendState::Sending(send)
                }
                SendState::Sending(send) => {
                    let tx_hash = match ready!(send.poll(cx)) {
                        Ok(tx_hash) => tx_hash,
                        Err(err) => return Poll::Ready(Err(err.into())),
                    };

                    let confirm = match resolve.take().expect("confirmation called more than once")
                    {
                        ResolveCondition::Pending => {
                            return Poll::Ready(Ok(TransactionResult::Hash(tx_hash)))
                        }
                        ResolveCondition::Confirmed(params) => {
                            ConfirmFuture::new(&web3, tx_hash, params)
                        }
                    };

                    SendState::Confirming(confirm)
                }
                SendState::Confirming(confirm) => {
                    return confirm.poll(cx).map(|result| {
                        let tx = result?;
                        match tx.status {
                            Some(U64([1])) => Ok(TransactionResult::Receipt(tx)),
                            _ => Err(ExecutionError::Failure(tx.transaction_hash)),
                        }
                    })
                }
            };

            self.state = next_state;
        }
    }
}
