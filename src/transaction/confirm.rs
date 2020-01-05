//! Transaction confirmation implementation. This is a re-implementation of
//! `web3` confirmation future to fix issues with development nodes like Ganache
//! where the transaction gets mined right away, so waiting for 1 confirmation
//! would require another transaction to be sent so a new block could mine.
//! Additionally, waiting for 0 confirmations in `web3` means that the tx is
//! just sent to the mem-pool but does not wait for it to get mined. Hopefully
//! some of this can move upstream into the `web3` crate.

#![allow(missing_docs)]

use crate::errors::ExecutionError;
use crate::future::CompatCallFuture;
use futures::future::{Either, TryJoin};
use futures::ready;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::num::NonZeroUsize;
use web3::api::Web3;
use web3::types::{TransactionReceipt, H256, U256};
use web3::Transport;

pub struct ConfirmFuture<T: Transport> {
    web3: Web3<T>,
    tx: H256,
    confirmations: NonZeroUsize,
    state: ConfirmState<T>,
}

impl<T: Transport> ConfirmFuture<T> {
    pub fn new(web3: Web3<T>, tx: H256, confirmations: usize) -> Either<
}

impl<T: Transport> ConfirmFuture<T> {
    fn state_mut(self: Pin<&mut Self>) -> &mut ConfirmState<T> {
        // NOTE: this is safe as the `state` field does not need to be pinned
        unsafe { &mut self.get_unchecked_mut().state }
    }
}

impl<T: Transport> Future for ConfirmFuture<T> {
    type Output = Result<TransactionResult, ExecutionError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        loop {
            match self.as_mut().state_mut() {
                ConfirmState::Checking(ref mut check) => {
                    let (block_num, tx) = match ready!(Pin::new(check).poll(cx)) {
                        Ok(result) => result,
                        Err(err) => return Poll::Ready(Err(err.into())),
                    };
                    let tx_block_num = tx.and_then(|tx| tx.block_number).unwrap_or(block_num);

                    if block_num + 1 >= tx_block_num + self.confirmations.get() {
                        return Poll::Ready
                    }
                    todo!()
                }
                ConfirmState::WaitingForBlocks(ref mut wait) => todo!(),
            }
        }
    }
}

enum ConfirmState<T: Transport> {
    Checking(CheckFuture<T>),
    WaitingForBlocks(WaitForBlocksFuture<T>),
}

type CheckFuture<T> = TryJoin<CompatCallFuture<T, U256>, CompatCallFuture<T, TransactionReceipt>>;

struct WaitForBlocksFuture<T>(T);

pub enum TransactionResult {
    Hash(H256),
    Receipt(TransactionReceipt),
}
