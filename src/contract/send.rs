//! Implementation for sending a transaction to a contract.

use crate::transaction::{Account, ExecuteFuture, TransactionBuilder};
use web3::api::Web3;
use web3::types::{Address, Bytes, U256};
use web3::Transport;

/// Data used for building a contract transaction that modifies the blockchain.
/// These transactions can either be sent to be signed locally by the node or can
/// be signed offline.
#[derive(Debug, Clone)]
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

    /// Extract inner `TransactionBuilder` from this `SendBuilder`. This exposes
    /// `TransactionBuilder` only APIs such as `estimate_gas` and `build`.
    pub fn into_inner(self) -> TransactionBuilder<T> {
        self.0
    }

    /// Sign (if required) and execute the transaction. Returns the transaction
    /// hash that can be used to retrieve transaction information.
    pub fn execute(self) -> ExecuteFuture<T> {
        self.0.execute()
    }
}
