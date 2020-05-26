//! Transaction confirmation implementation. This is a re-implementation of
//! `web3` confirmation future to fix issues with development nodes like Ganache
//! where the transaction gets mined right away, so waiting for 1 confirmation
//! would require another transaction to be sent so a new block could mine.
//! Additionally, waiting for 0 confirmations in `web3` means that the tx is
//! just sent to the mem-pool but does not wait for it to get mined. Hopefully
//! some of this can move upstream into the `web3` crate.

use crate::errors::ExecutionError;
use futures::compat::Future01CompatExt;
use futures_timer::Delay;
use std::time::Duration;
use web3::api::Web3;
use web3::futures::Stream as _;
use web3::types::{TransactionReceipt, H256, U64};
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
    pub block_timeout: Option<usize>,
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
pub const DEFAULT_BLOCK_TIMEOUT: Option<usize> = Some(25);

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

/// Waits for a transaction to be confirmed.
pub async fn wait_for_confirmation<T: Transport>(
    web3: &Web3<T>,
    tx: H256,
    params: ConfirmParams,
) -> Result<TransactionReceipt, ExecutionError> {
    let context = ConfirmationContext { web3, tx, params };

    let mut latest_block = web3.eth().block_number().compat().await?;
    let starting_block = latest_block;
    loop {
        let (target_block, remaining_confirmations) = match context.check(latest_block).await? {
            Check::Confirmed(tx) => return Ok(tx),
            Check::Pending {
                target_block,
                remaining_confirmations,
            } => (target_block, remaining_confirmations),
        };

        if let Some(block_timeout) = context.params.block_timeout {
            let elapsed_blocks = latest_block.saturating_sub(starting_block);
            if elapsed_blocks > U64::from(block_timeout) {
                return Err(ExecutionError::ConfirmTimeout);
            }
        }

        context
            .wait_for_blocks(target_block, remaining_confirmations)
            .await?;
        latest_block = target_block;
    }
}

/// The state used for waiting for a transaction confirmation.
#[derive(Debug)]
struct ConfirmationContext<'a, T: Transport> {
    web3: &'a Web3<T>,
    /// The transaction hash that is being confirmed.
    tx: H256,
    /// The confirmation parameters (like number of confirming blocks to wait
    /// for and polling interval).
    params: ConfirmParams,
}

impl<T: Transport> ConfirmationContext<'_, T> {
    /// Checks if the transaction is confirmed.
    ///
    /// Accepts an optional block number parameter to avoid re-querying the
    /// current block if it is already known.
    async fn check(&self, latest_block: U64) -> Result<Check, ExecutionError> {
        let tx = self
            .web3
            .eth()
            .transaction_receipt(self.tx)
            .compat()
            .await?;

        let (target_block, remaining_confirmations) =
            match tx.and_then(|tx| Some((tx.block_number?, tx))) {
                Some((tx_block, tx)) => {
                    let target_block = tx_block + self.params.confirmations;
                    let remaining_confirmations = target_block.saturating_sub(latest_block);

                    if remaining_confirmations.is_zero() {
                        return Ok(Check::Confirmed(tx));
                    }

                    (target_block, remaining_confirmations.as_usize())
                }
                None => {
                    let remaining_confirmations = self.params.confirmations + 1;
                    (
                        latest_block + remaining_confirmations,
                        remaining_confirmations,
                    )
                }
            };

        Ok(Check::Pending {
            target_block,
            remaining_confirmations,
        })
    }

    /// Waits for blocks to be mined. This method tries to use a block filter to
    /// wait for a certain number of blocks to be mined. If that fails, it falls
    /// back to polling the latest block number to wait until a target block
    /// number is reached.
    async fn wait_for_blocks(
        &self,
        target_block: U64,
        block_count: usize,
    ) -> Result<(), ExecutionError> {
        if self
            .wait_for_blocks_with_filter(target_block, block_count)
            .await
            .is_err()
        {
            // NOTE: In the case we fail to create a filter (usually because the
            //   node doesn't support filters like Infura over HTTPS) or we fail
            //   to query the filter (node is behind a load balancer or cleaned
            //   up the filter) then fall back to polling.
            self.wait_for_blocks_with_polling(target_block).await?;
        }

        Ok(())
    }

    /// Waits for a certain number of blocks to be mined using a block filter.
    async fn wait_for_blocks_with_filter(
        &self,
        target_block: U64,
        mut block_count: usize,
    ) -> Result<(), ExecutionError> {
        let mut block_stream = self
            .web3
            .eth_filter()
            .create_blocks_filter()
            .compat()
            .await?
            .stream(self.params.poll_interval);

        loop {
            while block_count > 0 {
                block_stream = block_stream
                    .into_future()
                    .compat()
                    .await
                    .map_err(|(err, _)| err)?
                    .1;
                block_count -= 1;
            }

            let latest_block = self.web3.eth().block_number().compat().await?;
            if latest_block >= target_block {
                return Ok(());
            }

            block_count = (target_block - latest_block).as_usize();
        }
    }

    /// Waits for the block chain to reach the target height by polling the
    /// current latest block.
    async fn wait_for_blocks_with_polling(&self, target_block: U64) -> Result<(), ExecutionError> {
        while {
            delay(self.params.poll_interval).await;
            let latest_block = self.web3.eth().block_number().compat().await?;

            latest_block < target_block
        } {}

        Ok(())
    }
}

/// The result of checking a transaction confirmation.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
enum Check {
    /// The transaction is confirmed with a transaction receipt.
    Confirmed(TransactionReceipt),
    /// The transaction is not yet confirmed, and requires additional block
    /// confirmations.
    Pending {
        target_block: U64,
        remaining_confirmations: usize,
    },
}

/// Create a new delay that may resolve immediately when delayed for a zero
/// duration.
///
/// This method is used so that unit tests resolve immediately, as the `Delay`
/// future always returns `Poll::Pending` at least once, even with a delay or
/// zero.
async fn delay(duration: Duration) {
    const ZERO_DURATION: Duration = Duration::from_secs(0);

    if duration != ZERO_DURATION {
        Delay::new(duration).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::prelude::*;
    use serde_json::Value;
    use web3::types::H2048;

    fn generate_tx_receipt<U: Into<U64>>(hash: H256, block_num: U) -> Value {
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
        transport.add_response(json!("0x2"));
        // check transaction was mined
        transport.add_response(generate_tx_receipt(hash, 2));

        let confirm = wait_for_confirmation(&web3, hash, ConfirmParams::mined())
            .immediate()
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

        let confirm = wait_for_confirmation(&web3, hash, ConfirmParams::mined())
            .immediate()
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
        transport.add_response(json!("0x5"));
        // check confirmation again - transaction mined on block 3 instead of 2
        transport.add_response(generate_tx_receipt(hash, 3));
        // needs to wait 1 more block - creating filter again and polling
        transport.add_response(json!("0xf1"));
        transport.add_response(json!([H256::repeat_byte(6)]));
        transport.add_response(json!("0x6"));
        // check confirmation one last time
        transport.add_response(generate_tx_receipt(hash, 3));

        let confirm = wait_for_confirmation(&web3, hash, ConfirmParams::with_confirmations(3))
            .immediate()
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
        // check transaction was mined
        transport.add_response(generate_tx_receipt(hash, 2));

        let confirm = wait_for_confirmation(&web3, hash, ConfirmParams::with_confirmations(1))
            .immediate()
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
    fn confirmations_with_polling_on_filter_error() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let hash = H256::repeat_byte(0xff);

        // transaction pending
        transport.add_response(json!("0x1"));
        transport.add_response(json!(null));
        // filter created
        transport.add_response(json!("0xf0"));
        // polled block filter until failure
        transport.add_response(json!([H256::repeat_byte(2)]));
        transport.add_response(json!({ "error": "filter not found" }));
        // poll block number until new block is found
        transport.add_response(json!("0x2"));
        transport.add_response(json!("0x2"));
        transport.add_response(json!("0x2"));
        transport.add_response(json!("0x3"));
        // check transaction was mined
        transport.add_response(generate_tx_receipt(hash, 2));

        let confirm = wait_for_confirmation(&web3, hash, ConfirmParams::with_confirmations(1))
            .immediate()
            .expect("transaction confirmation failed");

        assert_eq!(confirm.transaction_hash, hash);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_request("eth_newBlockFilter", &[]);
        transport.assert_request("eth_getFilterChanges", &[json!("0xf0")]);
        transport.assert_request("eth_getFilterChanges", &[json!("0xf0")]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_no_more_requests();
    }

    #[test]
    fn confirmations_with_polling_and_skipped_blocks() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let hash = H256::repeat_byte(0xff);

        // transaction pending
        transport.add_response(json!("0x1"));
        transport.add_response(json!(null));
        // filter created not supported
        transport.add_response(json!({ "error": "eth_newBlockFilter not supported" }));
        // poll block number which skipped 2
        transport.add_response(json!("0x4"));
        // check transaction was mined
        transport.add_response(generate_tx_receipt(hash, 2));

        let confirm = wait_for_confirmation(&web3, hash, ConfirmParams::with_confirmations(1))
            .immediate()
            .expect("transaction confirmation failed");

        assert_eq!(confirm.transaction_hash, hash);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_request("eth_newBlockFilter", &[]);
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
        transport.add_response(json!("0x3"));
        // check confirmation again - transaction mined on block 3
        transport.add_response(generate_tx_receipt(hash, 3));
        // needs to wait 1 more block - creating filter again and polling
        transport.add_response(json!("0xf1"));
        transport.add_response(json!([H256::repeat_byte(4)]));
        transport.add_response(json!("0x4"));
        // check confirmation - reorg happened, tx mined on block 4!
        transport.add_response(generate_tx_receipt(hash, 4));
        // wait for another block
        transport.add_response(json!("0xf2"));
        transport.add_response(json!([H256::repeat_byte(5)]));
        transport.add_response(json!("0x5"));
        // check confirmation - and we are satisfied.
        transport.add_response(generate_tx_receipt(hash, 4));

        let confirm = wait_for_confirmation(&web3, hash, ConfirmParams::with_confirmations(1))
            .immediate()
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
        transport.add_response(json!("0x4"));
        // check confirmation again - transaction mined on block 3
        transport.add_response(generate_tx_receipt(hash, 3));
        // needs to wait 1 more block - creating filter again and polling
        transport.add_response(json!("0xf1"));
        transport.add_response(json!([H256::repeat_byte(5)]));
        transport.add_response(json!("0x4"));
        // reorg happened and block 4 was replaced - filter polled for more blocks
        transport.add_response(json!([H256::repeat_byte(6)]));
        transport.add_response(json!("0x5"));
        // check confirmation - and we are satisfied.
        transport.add_response(generate_tx_receipt(hash, 3));

        let confirm = wait_for_confirmation(&web3, hash, ConfirmParams::with_confirmations(2))
            .immediate()
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
        transport.assert_request("eth_getFilterChanges", &[json!("0xf1")]);
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
        let timeout = params
            .block_timeout
            .expect("default confirm parameters have a block timeout")
            + 1;

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

        let confirm = wait_for_confirmation(&web3, hash, params).immediate();

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
