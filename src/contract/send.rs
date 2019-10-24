//! Implementation for sending a transaction to a contract.

use crate::transaction::{
    Account, BuildFuture, EstimateGasFuture, ExecuteConfirmFuture, ExecuteFuture,
    TransactionBuilder,
};
use std::time::Duration;
use web3::api::Web3;
use web3::types::{Address, Bytes, U256};
use web3::Transport;

/// Data used for building a contract transaction that modifies the blockchain.
/// These transactions can either be sent to be signed locally by the node or can
/// be signed offline.
pub struct SendBuilder<T: Transport>(TransactionBuilder<T>);

impl<T: Transport> SendBuilder<T> {
    /// Creates a new builder for a transaction.
    pub fn new(web3: Web3<T>, address: Address, data: Bytes) -> SendBuilder<T> {
        SendBuilder(TransactionBuilder::new(web3).to(address).data(data))
    }

    /// Specify the signing method to use for the transaction, if not specified
    /// the the transaction will be locally signed with the default user.
    pub fn from(self, value: Account) -> SendBuilder<T> {
        SendBuilder(self.0.from(value))
    }

    /// Secify amount of gas to use, if not specified then a gas estimate will
    /// be used.
    pub fn gas(self, value: U256) -> SendBuilder<T> {
        SendBuilder(self.0.gas(value))
    }

    /// Specify the gas price to use, if not specified then the estimated gas
    /// price will be used.
    pub fn gas_price(self, value: U256) -> SendBuilder<T> {
        SendBuilder(self.0.gas(value))
    }

    /// Specify what how much ETH to transfer with the transaction, if not
    /// specified then no ETH will be sent.
    pub fn value(self, value: U256) -> SendBuilder<T> {
        SendBuilder(self.0.gas(value))
    }

    /// Specify the nonce for the transation, if not specified will use the
    /// current transaction count for the signing account.
    pub fn nonce(self, value: U256) -> SendBuilder<T> {
        SendBuilder(self.0.gas(value))
    }

    /// Estimate the gas required for this transaction.
    pub fn estimate_gas(self) -> EstimateGasFuture<T> {
        self.0.estimate_gas()
    }

    /// Build a prepared transaction that is ready to send.
    pub fn build(self) -> BuildFuture<T> {
        self.0.build()
    }

    /// Sign (if required) and execute the transaction. Returns the transaction
    /// hash that can be used to retrieve transaction information.
    pub fn execute(self) -> ExecuteFuture<T> {
        self.0.execute()
    }

    /// Execute a transaction and wait for confirmation. Returns the transaction
    /// receipt for inspection.
    pub fn execute_and_confirm(
        self,
        poll_interval: Duration,
        confirmations: usize,
    ) -> ExecuteConfirmFuture<T> {
        self.0.execute_and_confirm(poll_interval, confirmations)
    }
}
