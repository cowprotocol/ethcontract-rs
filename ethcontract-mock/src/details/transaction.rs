//! Common transaction types.

use ethcontract::{Address, H256, U256};

/// Basic transaction parameters.
pub struct Transaction {
    pub from: Address,
    pub to: Address,
    pub nonce: U256,
    pub gas: U256,
    pub gas_price: U256,
    pub value: U256,
    pub data: Vec<u8>,
    pub hash: H256,
}

/// Transaction execution result.
pub struct TransactionResult {
    /// Result of a method call, error if call is aborted.
    pub result: Result<Vec<u8>, String>,

    /// How many blocks should be mined on top of transaction's block
    /// for confirmation to be successful.
    pub confirmations: u64,
}
