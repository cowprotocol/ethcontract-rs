//! Transaction confirmation implementation. This is a re-implementation of
//! `web3` confirmation future to fix issues with development nodes like Ganache
//! where the transaction gets mined right away, so waiting for 1 confirmation
//! would require another transaction to be sent so a new block could mine.
//! Additionally, waiting for 0 confirmations in `web3` means that the tx is
//! just sent to the mem-pool but does not wait for it to get mined. Hopefully
//! some of this can move upstream into the `web3` crate.

use crate::errors::ExecutionError;
use crate::transaction::TransactionResult;
use futures_timer::Delay;
use std::cmp::min;
use std::time::Duration;
use web3::api::Web3;
use web3::types::{TransactionReceipt, H256, U64};
use web3::Transport;

/// A struct with the confirmation parameters.
#[derive(Clone, Debug)]
#[must_use = "confirm parameters do nothing unless waited for"]
pub struct ConfirmParams {
    /// The number of blocks to confirm the transaction with. This is the number
    /// of blocks mined on top of the block where the transaction was mined.
    /// This means that, for example, to just wait for the transaction to be
    /// mined, then the number of confirmations should be 0. Positive non-zero
    /// values indicate that extra blocks should be waited for on top of the
    /// block where the transaction was mined.
    pub confirmations: usize,
    /// Minimal delay between consecutive `eth_blockNumber` calls.
    /// We wait for transaction confirmation by polling node for latest
    /// block number. We use exponential backoff to control how often
    /// we poll the node.
    pub poll_interval_min: Duration,
    /// Maximal delay between consecutive `eth_blockNumber` calls.
    pub poll_interval_max: Duration,
    /// Factor, by which the delay between consecutive `eth_blockNumber`
    /// calls is multiplied after each call.
    pub poll_interval_factor: f32,
    /// The maximum number of blocks to wait for a transaction to get confirmed.
    pub block_timeout: Option<usize>,
}

/// Default minimal delay between polling the node for transaction confirmation.
#[cfg(not(test))]
const DEFAULT_POLL_INTERVAL_MIN: Duration = Duration::from_millis(250);
#[cfg(test)]
const DEFAULT_POLL_INTERVAL_MIN: Duration = Duration::from_millis(0);

/// Default maximal delay between polling the node for transaction confirmation.
#[cfg(not(test))]
const DEFAULT_POLL_INTERVAL_MAX: Duration = Duration::from_millis(7000);
#[cfg(test)]
const DEFAULT_POLL_INTERVAL_MAX: Duration = Duration::from_millis(0);

/// Default factor for increasing delays between node polls.
#[cfg(not(test))]
const DEFAULT_POLL_INTERVAL_FACTOR: f32 = 1.7;
#[cfg(test)]
const DEFAULT_POLL_INTERVAL_FACTOR: f32 = 0.0;

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
            poll_interval_min: DEFAULT_POLL_INTERVAL_MIN,
            poll_interval_max: DEFAULT_POLL_INTERVAL_MAX,
            poll_interval_factor: DEFAULT_POLL_INTERVAL_FACTOR,
            block_timeout: DEFAULT_BLOCK_TIMEOUT,
        }
    }

    /// Set new value for [`confirmations`].
    ///
    /// [`confirmations`]: #structfield.confirmations
    #[inline]
    pub fn confirmations(mut self, confirmations: usize) -> Self {
        self.confirmations = confirmations;
        self
    }

    /// Set new values for exponential backoff settings.
    #[inline]
    pub fn poll_interval(mut self, min: Duration, max: Duration, factor: f32) -> Self {
        self.poll_interval_min = min;
        self.poll_interval_max = max;
        self.poll_interval_factor = factor;
        self
    }

    /// Set new value for [`poll_interval_min`].
    ///
    /// [`poll_interval_min`]: #structfield.poll_interval_min
    #[inline]
    pub fn poll_interval_min(mut self, poll_interval_min: Duration) -> Self {
        self.poll_interval_min = poll_interval_min;
        self
    }

    /// Set new value for [`poll_interval_max`].
    ///
    /// [`poll_interval_max`]: #structfield.poll_interval_max
    #[inline]
    pub fn poll_interval_max(mut self, poll_interval_max: Duration) -> Self {
        self.poll_interval_max = poll_interval_max;
        self
    }

    /// Set new value for [`poll_interval_factor`].
    ///
    /// [`poll_interval_factor`]: #structfield.poll_interval_factor
    #[inline]
    pub fn poll_interval_factor(mut self, poll_interval_factor: f32) -> Self {
        self.poll_interval_factor = poll_interval_factor;
        self
    }

    /// Set new value for [`block_timeout`].
    ///
    /// [`block_timeout`]: #structfield.block_timeout
    #[inline]
    pub fn block_timeout(mut self, block_timeout: Option<usize>) -> Self {
        self.block_timeout = block_timeout;
        self
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
    let mut latest_block = None;
    let mut context = ConfirmationContext {
        web3,
        tx,
        params,
        starting_block: None,
    };

    loop {
        let target_block = match context.check(latest_block).await? {
            Check::Confirmed(tx) => return Ok(tx),
            Check::Pending(target_block) => target_block,
        };

        latest_block = Some(context.wait_for_blocks(target_block).await?);
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
    /// The current block number when confirmation started. This is used for
    /// timeouts.
    starting_block: Option<U64>,
}

impl<T: Transport> ConfirmationContext<'_, T> {
    /// Checks if the transaction is confirmed.
    ///
    /// Accepts an optional block number parameter to avoid re-querying the
    /// current block if it is already known.
    async fn check(&mut self, latest_block: Option<U64>) -> Result<Check, ExecutionError> {
        let latest_block = match latest_block {
            Some(value) => value,
            None => self.web3.eth().block_number().await?,
        };
        let tx = self.web3.eth().transaction_receipt(self.tx).await?;

        let (target_block, tx_result) = match tx.and_then(|tx| Some((tx.block_number?, tx))) {
            Some((tx_block, tx)) => {
                let target_block = tx_block + self.params.confirmations;

                // This happens in two cases:
                // - we don't need additional confirmation, transaction receipt is enough,
                // - the transaction was mined before we queried `latest_block`, thus
                //   `latest_block >= tx_block`.
                if latest_block >= target_block || self.params.confirmations == 0 {
                    return Ok(Check::Confirmed(tx));
                }

                (target_block, TransactionResult::Receipt(tx))
            }
            None => {
                // We know that transaction was not mined at block `latest_block` because
                // we've fetched `latest_block` before we've fetched transaction receipt.
                // Thus, we need to wait at least one block after the `latest_block`,
                // and then `self.params.confirmations` blocks on top of that.
                (
                    latest_block + self.params.confirmations + 1,
                    TransactionResult::Hash(self.tx),
                )
            }
        };

        if let Some(block_timeout) = self.params.block_timeout {
            let starting_block = *self.starting_block.get_or_insert(latest_block);
            let remaining_blocks = target_block.saturating_sub(starting_block);

            if remaining_blocks > U64::from(block_timeout) {
                return Err(ExecutionError::ConfirmTimeout(Box::new(tx_result)));
            }
        }

        Ok(Check::Pending(target_block))
    }

    /// Waits for blocks to be mined. This method polls the latest block number
    /// and waits till the target block number is reached.
    ///
    /// This method returns the latest block number if it is known.
    async fn wait_for_blocks(&self, target_block: U64) -> Result<U64, ExecutionError> {
        let mut cur_delay = self.params.poll_interval_min;

        loop {
            delay(cur_delay).await;

            let latest_block = self.web3.eth().block_number().await?;
            if target_block <= latest_block {
                break Ok(latest_block);
            }

            cur_delay = min(
                cur_delay.mul_f32(self.params.poll_interval_factor),
                self.params.poll_interval_max,
            );
        }
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
    ///
    /// Contains estimated target block after which the transaction
    /// should be mined and confirmed. Note that waiting for that block does
    /// not guarantee that the transaction is confirmed. An additional
    /// check is required.
    Pending(U64),
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
        // poll for one block
        transport.add_response(json!("0x2"));
        transport.add_response(generate_tx_receipt(hash, 2));

        let confirm = wait_for_confirmation(&web3, hash, ConfirmParams::mined())
            .wait()
            .expect("transaction confirmation failed");

        assert_eq!(confirm.transaction_hash, hash);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
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
    fn confirm_mined_transaction_when_mining_is_delayed() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let hash = H256::repeat_byte(0xff);

        // transaction pending
        transport.add_response(json!("0x1"));
        transport.add_response(json!(null));
        // poll for one block
        transport.add_response(json!("0x2"));
        // transaction still not mined
        transport.add_response(json!(null));
        // poll for one more block
        transport.add_response(json!("0x3"));
        // now it's mined
        transport.add_response(generate_tx_receipt(hash, 2));

        let confirm = wait_for_confirmation(&web3, hash, ConfirmParams::mined())
            .wait()
            .expect("transaction confirmation failed");

        assert_eq!(confirm.transaction_hash, hash);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_no_more_requests();
    }

    #[test]
    fn confirm_mined_transaction_when_mining_is_ahead_of_us() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let hash = H256::repeat_byte(0xff);

        // current block is 2, tx was mined on block 1
        transport.add_response(json!("0x2"));
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
    fn confirmations_when_mining_is_way_ahead_of_us() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let hash = H256::repeat_byte(0xff);

        // current block is 3, tx was mined on block 1, so we can confirm it
        transport.add_response(json!("0x3"));
        transport.add_response(generate_tx_receipt(hash, 1));

        let confirm = wait_for_confirmation(&web3, hash, ConfirmParams::with_confirmations(2))
            .immediate()
            .expect("transaction confirmation failed");

        assert_eq!(confirm.transaction_hash, hash);
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

        let confirm = wait_for_confirmation(&web3, hash, ConfirmParams::with_confirmations(1))
            .wait()
            .expect("transaction confirmation failed");

        assert_eq!(confirm.transaction_hash, hash);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
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
    fn confirmations_with_polling_when_mining_is_slightly_ahead_of_us() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let hash = H256::repeat_byte(0xff);

        // current block is 2, tx was mined on block 1
        transport.add_response(json!("0x2"));
        transport.add_response(generate_tx_receipt(hash, 1));
        // still waiting for one more block
        transport.add_response(json!("0x2"));
        transport.add_response(json!("0x3"));
        transport.add_response(generate_tx_receipt(hash, 1));

        let confirm = wait_for_confirmation(&web3, hash, ConfirmParams::with_confirmations(2))
            .immediate()
            .expect("transaction confirmation failed");

        assert_eq!(confirm.transaction_hash, hash);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
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
        // poll block number which skipped 2
        transport.add_response(json!("0x4"));
        // check transaction was mined (`eth_blockNumber` request is reused)
        transport.add_response(generate_tx_receipt(hash, 2));

        let confirm = wait_for_confirmation(&web3, hash, ConfirmParams::with_confirmations(1))
            .immediate()
            .expect("transaction confirmation failed");

        assert_eq!(confirm.transaction_hash, hash);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_no_more_requests();
    }

    #[test]
    fn confirmations_with_polling_reorg_tx_receipt() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let hash = H256::repeat_byte(0xff);

        // transaction pending
        transport.add_response(json!("0x1"));
        transport.add_response(json!(null));
        // poll for 2 blocks
        transport.add_response(json!("0x2"));
        transport.add_response(json!("0x3"));
        // check confirmation again - transaction mined on block 3
        transport.add_response(generate_tx_receipt(hash, 3));
        // needs to wait 1 more block
        transport.add_response(json!("0x3"));
        transport.add_response(json!("0x4"));
        // check confirmation - reorg happened, tx mined on block 4!
        transport.add_response(generate_tx_receipt(hash, 4));
        // wait for another block
        transport.add_response(json!("0x5"));
        // check confirmation - and we are satisfied.
        transport.add_response(generate_tx_receipt(hash, 4));

        let confirm = wait_for_confirmation(&web3, hash, ConfirmParams::with_confirmations(1))
            .wait()
            .expect("transaction confirmation failed");

        assert_eq!(confirm.transaction_hash, hash);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_no_more_requests();
    }

    #[test]
    fn confirmation_timeout() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let hash = H256::repeat_byte(0xff);
        let params = ConfirmParams {
            confirmations: 3,
            block_timeout: Some(10),
            ..Default::default()
        };

        // Initial check
        transport.add_response(json!("0x0"));
        transport.add_response(json!(null));
        // Check again, at block 4
        transport.add_response(json!("0x4"));
        transport.add_response(json!(null));
        // Wait for more blocks
        // Final check at block 8, since the earliest the transaction can be
        // confirmed is at block 12 which is past the block timeout.
        transport.add_response(json!("0x8"));
        transport.add_response(json!(null));

        let confirm = wait_for_confirmation(&web3, hash, params).wait();

        assert!(
            match &confirm {
                Err(ExecutionError::ConfirmTimeout(tx)) => tx.is_hash(),
                _ => false,
            },
            "expected confirmation to time out but got {:?}",
            confirm
        );

        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(hash)]);
        transport.assert_no_more_requests();
    }
}
