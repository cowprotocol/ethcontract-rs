//! Transaction confirmation implementation. This is a re-implementation of
//! `web3` confirmation future to fix issues with development nodes like Ganache
//! where the transaction gets mined right away, so waiting for 1 confirmation
//! would require another transaction to be sent so a new block could mine.
//! Additionally, waiting for 0 confirmations in `web3` means that the tx is
//! just sent to the mem-pool but does not wait for it to get mined. Hopefully
//! some of this can move upstream into the `web3` crate.

use crate::errors::ExecutionError;
use crate::future::{CompatCallFuture, MaybeReady, Web3Unpin};
use futures::compat::{Compat01As03, Future01CompatExt};
use futures::future::{self, TryJoin};
use futures::ready;
use futures_timer::Delay;
use std::fmt::{self, Debug, Formatter};
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
///
/// Note that this is currently 7 seconds as this is what was chosen in `web3`
/// crate.
#[cfg(not(test))]
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(7);
#[cfg(test)]
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(0);

/// The default block timeout to use for confirming transactions.
pub const DEFAULT_BLOCK_TIMEOUT: usize = 25;

impl ConfirmParams {
    /// Create new confirmation parameters for just confirming that the
    /// transaction was mined but not confirmed with any extra blocks.
    pub fn mined() -> Self {
        ConfirmParams::with_confirmations(0)
    }

    /// Create new confirmation parameters from the specified number of extra
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
        ConfirmParams::mined()
    }
}

/// A future that resolves once a transaction is confirmed.
#[derive(Debug)]
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
    CreatingFilter(CompatCreateFilter<T, H256>, U256, u64),
    /// The future is waiting for new blocks to be mined and added to the chain
    /// so that the transaction can be confirmed the desired number of blocks.
    WaitingForBlocks(CompatFilterFuture<T, H256>),
    /// The future is waiting for a poll timeout. This state happens when the
    /// node does not support block filters for the given transport (like Infura
    /// over HTTPS) so we need to fallback to polling.
    PollDelay(Delay, U256),
    /// The future is checking that the current block number has reached a
    /// certain target after waiting the poll delay.
    PollCheckingBlockNumber(CompatCallFuture<T, U256>, U256),
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
                    MaybeReady::future(unpinned.web3.eth().block_number().compat()),
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
                        target_block_num,
                        remaining_confirmations.as_u64(),
                    )
                }
                ConfirmState::CreatingFilter(ref mut create_filter, target_block_num, count) => {
                    match ready!(Pin::new(create_filter).poll(cx)) {
                        Ok(filter) => ConfirmState::WaitingForBlocks(
                            filter
                                .stream(unpinned.params.poll_interval)
                                .skip(*count - 1)
                                .into_future()
                                .compat(),
                        ),
                        Err(_) => {
                            // NOTE: In the case we fail to create a filter
                            //   (usually because the node doesn't support
                            //   filters like Infura over HTTPS) then fall back
                            //   to polling.
                            ConfirmState::PollDelay(
                                Delay::new(unpinned.params.poll_interval),
                                *target_block_num,
                            )
                        }
                    }
                }
                ConfirmState::WaitingForBlocks(ref mut wait) => {
                    match ready!(Pin::new(wait).poll(cx)) {
                        Ok(_) => ConfirmState::Check,
                        Err((err, _)) => return Poll::Ready(Err(err.into())),
                    }
                }
                ConfirmState::PollDelay(ref mut delay, target_block_num) => {
                    ready!(Pin::new(delay).poll(cx));
                    ConfirmState::PollCheckingBlockNumber(
                        unpinned.web3.eth().block_number().compat(),
                        *target_block_num,
                    )
                }
                ConfirmState::PollCheckingBlockNumber(ref mut block_num, target_block_num) => {
                    let block_num = match ready!(Pin::new(block_num).poll(cx)) {
                        Ok(block_num) => block_num,
                        Err(err) => return Poll::Ready(Err(err.into())),
                    };

                    if block_num == *target_block_num {
                        ConfirmState::Checking(future::try_join(
                            MaybeReady::ready(Ok(block_num)),
                            unpinned
                                .web3
                                .eth()
                                .transaction_receipt(unpinned.tx)
                                .compat(),
                        ))
                    } else {
                        ConfirmState::PollDelay(
                            Delay::new(unpinned.params.poll_interval),
                            *target_block_num,
                        )
                    }
                }
            }
        }
    }
}

impl<T: Transport> Debug for ConfirmState<T> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            ConfirmState::Check => f.debug_tuple("Check").finish(),
            ConfirmState::Checking(_) => f.debug_tuple("Checking").finish(),
            ConfirmState::CreatingFilter(_, t, c) => {
                f.debug_tuple("CreatingFilter").field(t).field(c).finish()
            }
            ConfirmState::WaitingForBlocks(_) => f.debug_tuple("WaitingForBlocks").finish(),
            ConfirmState::PollDelay(d, t) => f.debug_tuple("PollDelay").field(d).field(t).finish(),
            ConfirmState::PollCheckingBlockNumber(_, t) => {
                f.debug_tuple("PollCheckingBlockNumber").field(t).finish()
            }
        }
    }
}

/// A type alias for a joined `eth_blockNumber` and `eth_getTransactionReceipt`
/// calls. Used when checking that the transaction has been confirmed by enough
/// blocks.
type CheckFuture<T> =
    TryJoin<MaybeReady<CompatCallFuture<T, U256>>, CompatCallFuture<T, Option<TransactionReceipt>>>;

/// A type alias for a future creating a `eth_newBlockFilter` filter.
type CompatCreateFilter<T, R> = Compat01As03<CreateFilter<T, R>>;

/// A type alias for a future that resolves once the block filter has received
/// a certain number of blocks.
type CompatFilterFuture<T, R> = Compat01As03<StreamFuture01<Skip01<FilterStream<T, R>>>>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::prelude::*;
    use serde_json::Value;
    use web3::types::H2048;

    fn generate_tx_receipt<U: Into<U256>>(hash: H256, block_num: U) -> Value {
        json!({
            "transactionHash": hash,
            "transactionIndex": "0x1",
            "blockNumber": block_num.into(),
            "blockHash": H256::zero(),
            "cumulativeGasUsed": "0x1337",
            "gasUsed": "0x1337",
            "logsBloom": H2048::zero(),
            "logs": [],
        })
    }

    #[test]
    fn confirm_mined_transaction() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let hash = H256::repeat_byte(0xff);

        // transaction pending
        transport.add_response(json!("0x1"));
        transport.add_response(json!(null));
        // filter created
        transport.add_response(json!("0xf0"));
        // polled block filter for 1 new block
        transport.add_response(json!([]));
        transport.add_response(json!([]));
        transport.add_response(json!([H256::repeat_byte(2)]));
        // check transaction was mined
        transport.add_response(json!("0x2"));
        transport.add_response(generate_tx_receipt(hash, 2));

        let confirm = ConfirmFuture::new(&web3, hash, ConfirmParams::mined())
            .wait()
            .expect("transaction confirmation failed");

        assert_eq!(confirm.transaction_hash, hash);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_request("eth_newBlockFilter", &[]);
        transport.assert_request("eth_getFilterChanges", &[json!("0xf0")]);
        transport.assert_request("eth_getFilterChanges", &[json!("0xf0")]);
        transport.assert_request("eth_getFilterChanges", &[json!("0xf0")]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_no_more_requests();
    }

    #[test]
    fn confirm_auto_mined_transaction() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let hash = H256::repeat_byte(0xff);

        transport.add_response(json!("0x1"));
        transport.add_response(generate_tx_receipt(hash, 1));

        let confirm = ConfirmFuture::new(&web3, hash, ConfirmParams::mined())
            .wait()
            .expect("transaction confirmation failed");

        assert_eq!(confirm.transaction_hash, hash);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_no_more_requests();
    }

    #[test]
    fn confirmations_with_filter() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let hash = H256::repeat_byte(0xff);

        // transaction pending
        transport.add_response(json!("0x1"));
        transport.add_response(json!(null));
        // filter created
        transport.add_response(json!("0xf0"));
        // polled block filter 4 times
        transport.add_response(json!([H256::repeat_byte(2), H256::repeat_byte(3)]));
        transport.add_response(json!([]));
        transport.add_response(json!([H256::repeat_byte(4)]));
        transport.add_response(json!([H256::repeat_byte(5)]));
        // check confirmation again - transaction mined on block 3 instead of 2
        transport.add_response(json!("0x5"));
        transport.add_response(generate_tx_receipt(hash, 3));
        // needs to wait 1 more block - creating filter again and polling
        transport.add_response(json!("0xf1"));
        transport.add_response(json!([H256::repeat_byte(6)]));
        // check confirmation one last time
        transport.add_response(json!("0x6"));
        transport.add_response(generate_tx_receipt(hash, 3));

        let confirm = ConfirmFuture::new(&web3, hash, ConfirmParams::with_confirmations(3))
            .wait()
            .expect("transaction confirmation failed");

        assert_eq!(confirm.transaction_hash, hash);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_request("eth_newBlockFilter", &[]);
        transport.assert_request("eth_getFilterChanges", &[json!("0xf0")]);
        transport.assert_request("eth_getFilterChanges", &[json!("0xf0")]);
        transport.assert_request("eth_getFilterChanges", &[json!("0xf0")]);
        transport.assert_request("eth_getFilterChanges", &[json!("0xf0")]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_request("eth_newBlockFilter", &[]);
        transport.assert_request("eth_getFilterChanges", &[json!("0xf1")]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_no_more_requests();
    }

    #[test]
    fn confirmations_with_polling() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let hash = H256::repeat_byte(0xff);

        // transaction pending
        transport.add_response(json!("0x1"));
        transport.add_response(json!(null));
        // filter created not supported
        transport.add_response(json!({ "error": "eth_newBlockFilter not supported" }));
        // poll block number until new block is found
        transport.add_response(json!("0x1"));
        transport.add_response(json!("0x1"));
        transport.add_response(json!("0x2"));
        transport.add_response(json!("0x2"));
        transport.add_response(json!("0x2"));
        transport.add_response(json!("0x3"));
        // check transaction was mined - note that the block number doesn't get
        // re-queried and is re-used from the polling loop.
        transport.add_response(generate_tx_receipt(hash, 2));

        let confirm = ConfirmFuture::new(&web3, hash, ConfirmParams::with_confirmations(1))
            .wait()
            .expect("transaction confirmation failed");

        assert_eq!(confirm.transaction_hash, hash);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_request("eth_newBlockFilter", &[]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_no_more_requests();
    }

    #[test]
    fn confirmations_with_reorg_tx_receipt() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let hash = H256::repeat_byte(0xff);

        // transaction pending
        transport.add_response(json!("0x1"));
        transport.add_response(json!(null));
        // filter created - poll for 2 blocks
        transport.add_response(json!("0xf0"));
        transport.add_response(json!([H256::repeat_byte(2)]));
        transport.add_response(json!([H256::repeat_byte(3)]));
        // check confirmation again - transaction mined on block 3
        transport.add_response(json!("0x3"));
        transport.add_response(generate_tx_receipt(hash, 3));
        // needs to wait 1 more block - creating filter again and polling
        transport.add_response(json!("0xf1"));
        transport.add_response(json!([H256::repeat_byte(4)]));
        // check confirmation - reorg happened, tx mined on block 4!
        transport.add_response(json!("0x4"));
        transport.add_response(generate_tx_receipt(hash, 4));
        // wait for another block
        transport.add_response(json!("0xf2"));
        transport.add_response(json!([H256::repeat_byte(5)]));
        // check confirmation - and we are satisfied.
        transport.add_response(json!("0x5"));
        transport.add_response(generate_tx_receipt(hash, 4));

        let confirm = ConfirmFuture::new(&web3, hash, ConfirmParams::with_confirmations(1))
            .wait()
            .expect("transaction confirmation failed");

        assert_eq!(confirm.transaction_hash, hash);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_request("eth_newBlockFilter", &[]);
        transport.assert_request("eth_getFilterChanges", &[json!("0xf0")]);
        transport.assert_request("eth_getFilterChanges", &[json!("0xf0")]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_request("eth_newBlockFilter", &[]);
        transport.assert_request("eth_getFilterChanges", &[json!("0xf1")]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_request("eth_newBlockFilter", &[]);
        transport.assert_request("eth_getFilterChanges", &[json!("0xf2")]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_no_more_requests();
    }

    #[test]
    fn confirmations_with_reorg_blocks() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let hash = H256::repeat_byte(0xff);

        // transaction pending
        transport.add_response(json!("0x1"));
        transport.add_response(json!(null));
        // filter created - poll for 2 blocks
        transport.add_response(json!("0xf0"));
        transport.add_response(json!([H256::repeat_byte(2)]));
        transport.add_response(json!([H256::repeat_byte(3)]));
        transport.add_response(json!([H256::repeat_byte(4)]));
        // check confirmation again - transaction mined on block 3
        transport.add_response(json!("0x4"));
        transport.add_response(generate_tx_receipt(hash, 3));
        // needs to wait 1 more block - creating filter again and polling
        transport.add_response(json!("0xf1"));
        transport.add_response(json!([H256::repeat_byte(5)]));
        // check confirmation - reorg happened and block 4 was replaced
        transport.add_response(json!("0x4"));
        transport.add_response(generate_tx_receipt(hash, 3));
        // wait for another block
        transport.add_response(json!("0xf2"));
        transport.add_response(json!([H256::repeat_byte(6)]));
        // check confirmation - and we are satisfied.
        transport.add_response(json!("0x5"));
        transport.add_response(generate_tx_receipt(hash, 3));

        let confirm = ConfirmFuture::new(&web3, hash, ConfirmParams::with_confirmations(2))
            .wait()
            .expect("transaction confirmation failed");

        assert_eq!(confirm.transaction_hash, hash);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_request("eth_newBlockFilter", &[]);
        transport.assert_request("eth_getFilterChanges", &[json!("0xf0")]);
        transport.assert_request("eth_getFilterChanges", &[json!("0xf0")]);
        transport.assert_request("eth_getFilterChanges", &[json!("0xf0")]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_request("eth_newBlockFilter", &[]);
        transport.assert_request("eth_getFilterChanges", &[json!("0xf1")]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_request("eth_newBlockFilter", &[]);
        transport.assert_request("eth_getFilterChanges", &[json!("0xf2")]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_no_more_requests();
    }

    #[test]
    fn confirmation_timeout() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let hash = H256::repeat_byte(0xff);
        let params = ConfirmParams::mined();
        let timeout = params.block_timeout + 1;

        // wait for the transaction a total of block timeout + 1 times
        for i in 0..timeout {
            let block_num = format!("0x{:x}", i + 1);
            let filter_id = format!("0xf{:x}", i);

            // transaction is pending
            transport.add_response(json!(block_num));
            transport.add_response(json!(null));
            transport.add_response(json!(filter_id));
            transport.add_response(json!([H256::repeat_byte(2)]));
        }

        let block_num = format!("0x{:x}", timeout + 1);
        transport.add_response(json!(block_num));
        transport.add_response(json!(null));

        let confirm = ConfirmFuture::new(&web3, hash, params).wait();

        assert!(
            match &confirm {
                Err(ExecutionError::ConfirmTimeout) => true,
                _ => false,
            },
            "expected confirmation to time out but got {:?}",
            confirm
        );

        for i in 0..timeout {
            let filter_id = format!("0xf{:x}", i);

            transport.assert_request("eth_blockNumber", &[]);
            transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
            transport.assert_request("eth_newBlockFilter", &[]);
            transport.assert_request("eth_getFilterChanges", &[json!(filter_id)]);
        }

        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_no_more_requests();
    }
}
