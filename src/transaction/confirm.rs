//! Transaction confirmation implementation. This is a re-implementation of
//! `web3` confirmation future to fix issues with development nodes like Ganache
//! where the transaction gets mined right away, so waiting for 1 confirmation
//! would require another transaction to be sent so a new block could mine.
//! Additionally, waiting for 0 confirmations in `web3` means that the tx is
//! just sent to the mem-pool but does not wait for it to get mined. Hopefully
//! some of this can move upstream into the `web3` crate.

use crate::errors::ExecutionError;
use crate::future::{CompatCallFuture, Web3Unpin};
use futures::compat::{Compat01As03, Future01CompatExt};
use futures::future::{self, TryJoin};
use futures::ready;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use web3::api::{CreateFilter, FilterStream, Web3};
use web3::futures::stream::{Skip as Skip01, StreamFuture as StreamFuture01};
use web3::futures::Stream as Stream01;
use web3::types::{TransactionReceipt, H256, U256};
use web3::Transport;

/// A struct with the confirmation parameters.
#[derive(Clone, Debug)]
pub struct ConfirmParams {
    /// The number of blocks to confirm the transaction with. This is the number
    /// of blocks mined on top of the block where the transaction was mined.
    /// This means that, for example, to just wait for the transaction to be
    /// mined, then the number of confirmations should be 0. Positive non-zero
    /// values indicate that extra blocks should be waited for on top of the
    /// block where the transaction was mined.
    pub confirmations: usize,
    /// The polling interval. This is used as the interval between consecutive
    /// `eth_getFilterChanges` calls to get filter updates, or the interval to
    /// wait between confirmation checks in case filters are not supported by
    /// the node (for example when using Infura over HTTP(S)).
    pub poll_interval: Duration,
    /// The maximum number of blocks to wait for a transaction to get confirmed.
    pub block_timeout: usize,
}

/// The default poll interval to use for confirming transactions.
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// The default poll interval to use for confirming transactions.
pub const DEFAULT_BLOCK_TIMEOUT: usize = 25;

impl ConfirmParams {
    /// Create new confirmation options from the specified number of extra
    /// blocks to wait for with the default poll interval.
    pub fn with_confirmations(count: usize) -> Self {
        ConfirmParams {
            confirmations: count,
            poll_interval: DEFAULT_POLL_INTERVAL,
            block_timeout: DEFAULT_BLOCK_TIMEOUT,
        }
    }
}

impl Default for ConfirmParams {
    fn default() -> Self {
        ConfirmParams::with_confirmations(0)
    }
}

/// A future that resolves once a transaction is confirmed.
pub struct ConfirmFuture<T: Transport> {
    web3: Web3Unpin<T>,
    /// The transaction hash that is being confirmed.
    tx: H256,
    /// The confirmation parameters (like number of confirming blocks to wait
    /// for and polling interval).
    params: ConfirmParams,
    /// The current block number when confirmation started. This is used for
    /// timeouts.
    starting_block_num: Option<U256>,
    /// The current state of the confirmation.
    state: ConfirmState<T>,
}

/// The state of the confirmation future.
enum ConfirmState<T: Transport> {
    /// The future is in the state where it needs to setup the checking future
    /// to see if the confirmation is complete. This is used as a intermediate
    /// state that doesn't actually wait for anything and immediately proceeds
    /// to the `Checking` state.
    Check,
    /// The future is waiting for the block number and transaction receipt to
    /// make sure that enough blocks have passed since the transaction was
    /// mined. Note that the transaction receipt is retrieved everytime in case
    /// of ommered blocks.
    Checking(CheckFuture<T>),
    /// The future is waiting for the block filter to be created so that it can
    /// wait for blocks to go by.
    CreatingFilter(CompatCreateFilter<T, H256>, u64),
    /// The future is waiting for new blocks to be mined and added to the chain
    /// so that the transaction can be confirmed the desired number of blocks.
    WaitingForBlocks(CompatFilterFuture<T, H256>),
    /// The future is waiting for a poll timeout. This state happens when the
    /// node does not support block filters for the given transport (like Infura
    /// over HTTPS) so we need to fallback to polling.
    WaitingForPollTimeout,
}

impl<T: Transport> ConfirmFuture<T> {
    /// Create a new `ConfirmFuture` with a `web3` provider for the specified
    /// transaction hash and with the specified parameters.
    pub fn new(web3: &Web3<T>, tx: H256, params: ConfirmParams) -> Self {
        ConfirmFuture {
            web3: web3.clone().into(),
            tx,
            params,
            starting_block_num: None,
            state: ConfirmState::Check,
        }
    }
}

impl<T: Transport> Future for ConfirmFuture<T> {
    type Output = Result<TransactionReceipt, ExecutionError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let unpinned = self.get_mut();
        loop {
            unpinned.state = match &mut unpinned.state {
                ConfirmState::Check => ConfirmState::Checking(future::try_join(
                    unpinned.web3.eth().block_number().compat(),
                    unpinned
                        .web3
                        .eth()
                        .transaction_receipt(unpinned.tx)
                        .compat(),
                )),
                ConfirmState::Checking(ref mut check) => {
                    let (block_num, tx) = match ready!(Pin::new(check).poll(cx)) {
                        Ok(result) => result,
                        Err(err) => return Poll::Ready(Err(err.into())),
                    };

                    // NOTE: If the transaction hasn't been mined, then assume
                    //   it will be picked up in the next block.
                    let tx_block_num = tx
                        .as_ref()
                        .and_then(|tx| tx.block_number)
                        .unwrap_or(block_num + 1);

                    let target_block_num = tx_block_num + unpinned.params.confirmations;
                    let remaining_confirmations = target_block_num.saturating_sub(block_num);

                    if remaining_confirmations.is_zero() {
                        // NOTE: It is safe to unwrap here since if tx is `None`
                        //   then the `remaining_confirmations` will always be
                        //   positive since `tx_block_num` will be a future
                        //   block.
                        return Poll::Ready(Ok(tx.unwrap()));
                    }

                    let starting_block_num = *unpinned.starting_block_num.get_or_insert(block_num);
                    if block_num.saturating_sub(starting_block_num)
                        > U256::from(unpinned.params.block_timeout)
                    {
                        return Poll::Ready(Err(ExecutionError::ConfirmTimeout));
                    }

                    ConfirmState::CreatingFilter(
                        unpinned.web3.eth_filter().create_blocks_filter().compat(),
                        remaining_confirmations.as_u64(),
                    )
                }
                ConfirmState::CreatingFilter(ref mut create_filter, count) => {
                    match ready!(Pin::new(create_filter).poll(cx)) {
                        Ok(filter) => ConfirmState::WaitingForBlocks(
                            filter
                                .stream(unpinned.params.poll_interval)
                                .skip(*count)
                                .into_future()
                                .compat(),
                        ),
                        Err(_) => {
                            // NOTE: In the case we fail to create a filter
                            //   (usually because the node doesn't support pub/
                            //   sub) then fall back to polling.
                            ConfirmState::WaitingForPollTimeout
                        }
                    }
                }
                ConfirmState::WaitingForBlocks(ref mut wait) => {
                    match ready!(Pin::new(wait).poll(cx)) {
                        Ok(_) => ConfirmState::Check,
                        Err((err, _)) => return Poll::Ready(Err(err.into())),
                    }
                }
                ConfirmState::WaitingForPollTimeout => todo!("polling is currently not supported"),
            }
        }
    }
}

/// A type alias for a joined `eth_blockNumber` and `eth_getTransactionReceipt`
/// calls. Used when checking that the transaction has been confirmed by enough
/// blocks.
type CheckFuture<T> =
    TryJoin<CompatCallFuture<T, U256>, CompatCallFuture<T, Option<TransactionReceipt>>>;

/// A type alias for a future creating a `eth_newBlockFilter` filter.
type CompatCreateFilter<T, R> = Compat01As03<CreateFilter<T, R>>;

/// A type alias for a future that resolves once the block filter has received
/// a certain number of blocks.
type CompatFilterFuture<T, R> = Compat01As03<StreamFuture01<Skip01<FilterStream<T, R>>>>;
