//! Implementation of a future for sending a transaction with optional
//! confirmation.

use crate::errors::ExecutionError;
use crate::transaction::confirm;
use crate::transaction::{ResolveCondition, Transaction, TransactionBuilder};
use web3::types::{TransactionReceipt, H256, U64};
use web3::Transport;

impl<T: Transport> TransactionBuilder<T> {
    /// Sign (if required) and send the transaction. Returns the transaction
    /// hash that can be used to retrieve transaction information.
    pub async fn send(mut self) -> Result<TransactionResult, ExecutionError> {
        let web3 = self.web3.clone();
        let resolve = self.resolve.take().unwrap_or_default();

        let tx = self.build().await?;
        let tx_hash = match tx {
            Transaction::Request(tx) => web3.eth().send_transaction(tx).await?,
            Transaction::Raw { bytes, hash } => {
                let node_hash = web3.eth().send_raw_transaction(bytes).await?;
                if node_hash != hash {
                    return Err(ExecutionError::UnexpectedTransactionHash);
                }
                hash
            }
        };

        let tx_receipt = match resolve {
            ResolveCondition::Pending => return Ok(TransactionResult::Hash(tx_hash)),
            ResolveCondition::Confirmed(params) => {
                confirm::wait_for_confirmation(&web3, tx_hash, params).await
            }
        }?;

        match tx_receipt.status {
            Some(U64([1])) => Ok(TransactionResult::Receipt(tx_receipt)),
            _ => Err(ExecutionError::Failure(Box::new(tx_receipt))),
        }
    }
}

/// Represents the result of a sent transaction that can either be a transaction
/// hash, in the case the transaction was not confirmed, or a full transaction
/// receipt if the `TransactionBuilder` was configured to wait for confirmation
/// blocks.
///
/// Note that the result will always be a `TransactionResult::Hash` if
/// `Confirm::Skip` was used and `TransactionResult::Receipt` if
/// `Confirm::Blocks` was used.
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum TransactionResult {
    /// A transaction hash, this variant happens if and only if confirmation was
    /// skipped.
    Hash(H256),
    /// A transaction receipt, this variant happens if and only if the
    /// transaction was configured to wait for confirmations.
    Receipt(TransactionReceipt),
}

impl TransactionResult {
    /// Returns true if the `TransactionResult` is a `Hash` variant, i.e. it is
    /// only a hash and does not contain the transaction receipt.
    pub fn is_hash(&self) -> bool {
        matches!(self, TransactionResult::Hash(_))
    }

    /// Get the transaction hash.
    pub fn hash(&self) -> H256 {
        match self {
            TransactionResult::Hash(hash) => *hash,
            TransactionResult::Receipt(tx) => tx.transaction_hash,
        }
    }

    /// Returns true if the `TransactionResult` is a `Receipt` variant, i.e. the
    /// transaction was confirmed and the full transaction receipt is available.
    pub fn is_receipt(&self) -> bool {
        self.as_receipt().is_some()
    }

    /// Extract a `TransactionReceipt` from the result. This will return `None`
    /// if the result is only a hash and the transaction receipt is not
    /// available.
    pub fn as_receipt(&self) -> Option<&TransactionReceipt> {
        match self {
            TransactionResult::Receipt(ref tx) => Some(tx),
            _ => None,
        }
    }
}
