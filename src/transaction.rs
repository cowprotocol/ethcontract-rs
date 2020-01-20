//! Implementation for setting up, signing, estimating gas and sending
//! transactions on the Ethereum network.

pub mod build;
pub mod confirm;

use crate::conv;
use crate::errors::ExecutionError;
use crate::future::CompatCallFuture;
use crate::transaction::build::BuildFuture;
use crate::transaction::confirm::{ConfirmFuture, ConfirmParams};
use ethsign::{Protected, SecretKey};
use futures::compat::Future01CompatExt;
use futures::ready;
use pin_project::{pin_project, project};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use web3::api::{Eth, Namespace, Web3};
use web3::helpers::{self, CallFuture};
use web3::types::{
    Address, Bytes, CallRequest, TransactionCondition, TransactionReceipt, TransactionRequest,
    H256, U256, U64,
};
use web3::Transport;

/// The account type used for signing the transaction.
#[derive(Clone, Debug)]
pub enum Account {
    /// Let the node sign for a transaction with an unlocked account.
    Local(Address, Option<TransactionCondition>),
    /// Do online signing with a locked account with a password.
    Locked(Address, Protected, Option<TransactionCondition>),
    /// Do offline signing with private key and optionally specify chain ID. If
    /// no chain ID is specified, then it will default to the network ID.
    Offline(SecretKey, Option<u64>),
}

impl Account {
    /// Returns the public address of an account.
    pub fn address(&self) -> Address {
        match self {
            Account::Local(address, _) => *address,
            Account::Locked(address, _, _) => *address,
            Account::Offline(key, _) => key.public().address().into(),
        }
    }
}

/// The condition on which a transaction's `SendFuture` gets resolved.
#[derive(Clone, Debug)]
pub enum ResolveCondition {
    /// The transaction's `SendFuture` gets resolved immediately after it was
    /// added to the pending transaction pool. This skips confirmation and
    /// provides no guarantees that the transaction was mined or confirmed.
    Pending,
    /// Wait for confirmation with the specified `ConfirmParams`. A confirmed
    /// transaction is always mined. There is a chance, however, that the block
    /// in which the transaction was mined becomes an ommer block. Confirming
    /// with a higher block count significantly decreases this probability.
    ///
    /// See `ConfirmParams` documentation for more details on the exact
    /// semantics confirmation.
    Confirmed(ConfirmParams),
}

impl Default for ResolveCondition {
    fn default() -> Self {
        ResolveCondition::Confirmed(Default::default())
    }
}

/// The gas price setting to use.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GasPrice {
    /// The standard estimated gas price from the node, this is usually the
    /// median gas price from the last few blocks. This is the default gas price
    /// used by transactions.
    Standard,
    /// A factor of the estimated gas price from the node. `GasPrice::Standard`
    /// is equivalent to `GasPrice::Factor(1.0)`.
    Factor(f64),
    /// Specify a specific gas price to use for the transaction. This will cause
    /// the transaction `SendFuture` to not query the node for a gas price
    /// estimation.
    Value(U256),
}

impl GasPrice {
    /// A low gas price. Using this may result in long confirmation times for
    /// transactions, or the transactions not being mined at all.
    pub fn low() -> Self {
        GasPrice::Factor(0.8)
    }

    /// A high gas price that usually results in faster mining times.
    /// transactions, or the transactions not being mined at all.
    pub fn fast() -> Self {
        GasPrice::Factor(6.0)
    }

    /// Returns `Some(value)` if the gas price is explicitly specified, `None`
    /// otherwise.
    pub fn value(&self) -> Option<U256> {
        match self {
            GasPrice::Value(value) => Some(*value),
            _ => None,
        }
    }

    /// Calculates the gas price to use based on the estimated gas price.
    fn calculate_price(&self, estimate: U256) -> U256 {
        match self {
            GasPrice::Standard => estimate,
            GasPrice::Factor(factor) => {
                // NOTE: U256 does not support floating point we we have to
                //   convert everything to floats to multiply the factor and
                //   then convert back. We are OK with the loss of precision
                //   here.
                let estimate_f = conv::u256_to_f64(estimate);
                conv::f64_to_u256(estimate_f * factor)
            }
            GasPrice::Value(value) => *value,
        }
    }
}

impl Default for GasPrice {
    fn default() -> Self {
        GasPrice::Standard
    }
}

impl From<U256> for GasPrice {
    fn from(value: U256) -> Self {
        GasPrice::Value(value)
    }
}

macro_rules! impl_gas_price_from_integer {
    ($($t:ty),* $(,)?) => {
        $(
            impl From<$t> for GasPrice {
                fn from(value: $t) -> Self {
                    GasPrice::Value(value.into())
                }
            }
        )*
    };
}

impl_gas_price_from_integer!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize,);

/// Represents a prepared and optionally signed transaction that is ready for
/// sending created by a `TransactionBuilder`.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Transaction {
    /// A structured transaction request to be signed locally by the node.
    Request(TransactionRequest),
    /// A signed raw transaction request.
    Raw(Bytes),
}

impl Transaction {
    /// Unwraps the transaction into a transaction request, returning None if the
    /// transaction is a raw transaction.
    pub fn request(self) -> Option<TransactionRequest> {
        match self {
            Transaction::Request(tx) => Some(tx),
            _ => None,
        }
    }

    /// Unwraps the transaction into its raw bytes, returning None if it is a
    /// transaction request.
    pub fn raw(self) -> Option<Bytes> {
        match self {
            Transaction::Raw(tx) => Some(tx),
            _ => None,
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
        match self {
            TransactionResult::Hash(_) => true,
            _ => false,
        }
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

/// Data used for building a transaction that modifies the blockchain. These
/// transactions can either be sent to be signed locally by the node or can be
/// signed offline.
#[derive(Clone, Debug)]
#[must_use = "transactions do nothing unless you `.build()` or `.send()` them"]
pub struct TransactionBuilder<T: Transport> {
    web3: Web3<T>,
    /// The sender of the transaction with the signing strategy to use. Defaults
    /// to locally signing on the node with the default acount.
    pub from: Option<Account>,
    /// The receiver of the transaction.
    pub to: Option<Address>,
    /// Optional gas amount to use for transaction. Defaults to estimated gas.
    pub gas: Option<U256>,
    /// Optional gas price to use for transaction. Defaults to estimated gas
    /// price from the node (i.e. `GasPrice::Standard`).
    pub gas_price: Option<GasPrice>,
    /// The ETH value to send with the transaction. Defaults to 0.
    pub value: Option<U256>,
    /// The data for the transaction. Defaults to empty data.
    pub data: Option<Bytes>,
    /// Optional nonce to use. Defaults to the signing account's current
    /// transaction count.
    pub nonce: Option<U256>,
    /// Optional resolve conditions. Defaults to waiting the transaction to be
    /// mined without any extra confirmation blocks.
    pub resolve: Option<ResolveCondition>,
}

impl<T: Transport> TransactionBuilder<T> {
    /// Creates a new builder for a transaction.
    pub fn new(web3: Web3<T>) -> Self {
        TransactionBuilder {
            web3,
            from: None,
            to: None,
            gas: None,
            gas_price: None,
            value: None,
            data: None,
            nonce: None,
            resolve: None,
        }
    }

    /// Specify the signing method to use for the transaction, if not specified
    /// the the transaction will be locally signed with the default user.
    pub fn from(mut self, value: Account) -> Self {
        self.from = Some(value);
        self
    }

    /// Specify the recepient of the transaction, if not specified the
    /// transaction will be sent to the 0 address (for deploying contracts).
    pub fn to(mut self, value: Address) -> Self {
        self.to = Some(value);
        self
    }

    /// Secify amount of gas to use, if not specified then a gas estimate will
    /// be used.
    pub fn gas(mut self, value: U256) -> Self {
        self.gas = Some(value);
        self
    }

    /// Specify the gas price to use, if not specified then the estimated gas
    /// price will be used.
    pub fn gas_price(mut self, value: GasPrice) -> Self {
        self.gas_price = Some(value);
        self
    }

    /// Specify what how much ETH to transfer with the transaction, if not
    /// specified then no ETH will be sent.
    pub fn value(mut self, value: U256) -> Self {
        self.value = Some(value);
        self
    }

    /// Specify the data to use for the transaction, if not specified, then empty
    /// data will be used.
    pub fn data(mut self, value: Bytes) -> Self {
        self.data = Some(value);
        self
    }

    /// Specify the nonce for the transation, if not specified will use the
    /// current transaction count for the signing account.
    pub fn nonce(mut self, value: U256) -> Self {
        self.nonce = Some(value);
        self
    }

    /// Specify the resolve condition, if not specified will default to waiting
    /// for the transaction to be mined (but not confirmed by any extra blocks).
    pub fn resolve(mut self, value: ResolveCondition) -> Self {
        self.resolve = Some(value);
        self
    }

    /// Specify the number of confirmations to use for the confirmation options.
    /// This is a utility method for specifying the resolve condition.
    pub fn confirmations(mut self, value: usize) -> Self {
        self.resolve = match self.resolve {
            Some(ResolveCondition::Confirmed(params)) => {
                Some(ResolveCondition::Confirmed(ConfirmParams {
                    confirmations: value,
                    ..params
                }))
            }
            _ => Some(ResolveCondition::Confirmed(
                ConfirmParams::with_confirmations(value),
            )),
        };
        self
    }

    /// Estimate the gas required for this transaction.
    pub fn estimate_gas(self) -> EstimateGasFuture<T> {
        EstimateGasFuture::from_builder(self)
    }

    /// Build a prepared transaction that is ready to send.
    pub fn build(self) -> BuildFuture<T> {
        BuildFuture::from_builder(self)
    }

    /// Sign (if required) and send the transaction. Returns the transaction
    /// hash that can be used to retrieve transaction information.
    pub fn send(self) -> SendFuture<T> {
        SendFuture::from_builder(self)
    }
}

/// Future for estimating gas for a transaction.
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[pin_project]
pub struct EstimateGasFuture<T: Transport>(#[pin] CompatCallFuture<T, U256>);

impl<T: Transport> EstimateGasFuture<T> {
    /// Create a instance from a `TransactionBuilder`.
    pub fn from_builder(builder: TransactionBuilder<T>) -> Self {
        let eth = builder.web3.eth();

        let from = builder.from.map(|account| account.address());
        let to = builder.to.unwrap_or_else(Address::zero);
        let request = CallRequest {
            from,
            to,
            gas: None,
            gas_price: None,
            value: builder.value,
            data: builder.data,
        };

        EstimateGasFuture::from_request(eth, request)
    }

    fn from_request(eth: Eth<T>, request: CallRequest) -> Self {
        // NOTE(nlordell): work around issue tomusdrw/rust-web3#290; while this
        //   bas been fixed in master, it has not been released yet
        EstimateGasFuture(
            CallFuture::new(
                eth.transport()
                    .execute("eth_estimateGas", vec![helpers::serialize(&request)]),
            )
            .compat(),
        )
    }
}

impl<T: Transport> Future for EstimateGasFuture<T> {
    type Output = Result<U256, ExecutionError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.project().0.poll(cx).map_err(ExecutionError::from)
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::prelude::*;
    use web3::types::H2048;

    #[test]
    fn tx_builder_estimate_gas() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let to = addr!("0x0123456789012345678901234567890123456789");

        transport.add_response(json!("0x42")); // estimate gas response
        let estimate_gas = TransactionBuilder::new(web3)
            .to(to)
            .value(42.into())
            .estimate_gas();

        transport.assert_request(
            "eth_estimateGas",
            &[json!({
                "to": to,
                "value": "0x2a",
            })],
        );
        transport.assert_no_more_requests();

        let estimate_gas = estimate_gas.immediate().expect("success");
        assert_eq!(estimate_gas, 0x42.into());
    }

    #[test]
    fn tx_send_local() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let from = addr!("0x9876543210987654321098765432109876543210");
        let to = addr!("0x0123456789012345678901234567890123456789");
        let hash = hash!("0x4242424242424242424242424242424242424242424242424242424242424242");

        transport.add_response(json!(hash)); // tansaction hash
        let tx = TransactionBuilder::new(web3)
            .from(Account::Local(from, Some(TransactionCondition::Block(100))))
            .to(to)
            .gas(1.into())
            .gas_price(2.into())
            .value(28.into())
            .data(Bytes(vec![0x13, 0x37]))
            .nonce(42.into())
            .resolve(ResolveCondition::Pending)
            .send()
            .immediate()
            .expect("transaction success");

        // assert that all the parameters are being used and that no extra
        // request was being sent (since no extra data from the node is needed)
        transport.assert_request(
            "eth_sendTransaction",
            &[json!({
                "from": from,
                "to": to,
                "gas": "0x1",
                "gasPrice": "0x2",
                "value": "0x1c",
                "data": "0x1337",
                "nonce": "0x2a",
                "condition": { "block": 100 },
            })],
        );
        transport.assert_no_more_requests();

        // assert the tx hash is what we expect it to be
        assert_eq!(tx.hash(), hash);
    }

    #[test]
    fn tx_send_with_confirmations() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let key = key!("0x0102030405060708091011121314151617181920212223242526272829303132");
        let chain_id = 77777;
        let tx_hash = H256::repeat_byte(0xff);

        transport.add_response(json!(tx_hash));
        transport.add_response(json!("0x1"));
        transport.add_response(json!(null));
        transport.add_response(json!("0xf0"));
        transport.add_response(json!([H256::repeat_byte(2), H256::repeat_byte(3)]));
        transport.add_response(json!("0x3"));
        transport.add_response(json!({
            "transactionHash": tx_hash,
            "transactionIndex": "0x1",
            "blockNumber": "0x2",
            "blockHash": H256::repeat_byte(3),
            "cumulativeGasUsed": "0x1337",
            "gasUsed": "0x1337",
            "logsBloom": H2048::zero(),
            "logs": [],
            "status": "0x1",
        }));

        let builder = TransactionBuilder::new(web3)
            .from(Account::Offline(key, Some(chain_id)))
            .to(Address::zero())
            .gas(0x1337.into())
            .gas_price(0x00ba_b10c.into())
            .nonce(0x42.into())
            .confirmations(1);
        let tx_raw = builder
            .clone()
            .build()
            .immediate()
            .expect("failed to sign transaction")
            .raw()
            .expect("offline transactions always build into raw transactions");
        let tx_receipt = builder
            .send()
            .immediate()
            .expect("send with confirmations failed");

        assert_eq!(tx_receipt.hash(), tx_hash);
        transport.assert_request("eth_sendRawTransaction", &[json!(tx_raw)]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(tx_hash)]);
        transport.assert_request("eth_newBlockFilter", &[]);
        transport.assert_request("eth_getFilterChanges", &[json!("0xf0")]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(tx_hash)]);
        transport.assert_no_more_requests();
    }

    #[test]
    fn tx_failure() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let key = key!("0x0102030405060708091011121314151617181920212223242526272829303132");
        let chain_id = 77777;
        let tx_hash = H256::repeat_byte(0xff);

        transport.add_response(json!(tx_hash));
        transport.add_response(json!("0x1"));
        transport.add_response(json!({
            "transactionHash": tx_hash,
            "transactionIndex": "0x1",
            "blockNumber": "0x1",
            "blockHash": H256::repeat_byte(1),
            "cumulativeGasUsed": "0x1337",
            "gasUsed": "0x1337",
            "logsBloom": H2048::zero(),
            "logs": [],
        }));

        let builder = TransactionBuilder::new(web3)
            .from(Account::Offline(key, Some(chain_id)))
            .to(Address::zero())
            .gas(0x1337.into())
            .gas_price(0x00ba_b10c.into())
            .nonce(0x42.into());
        let tx_raw = builder
            .clone()
            .build()
            .immediate()
            .expect("failed to sign transaction")
            .raw()
            .expect("offline transactions always build into raw transactions");
        let result = builder.send().immediate();

        assert!(
            match &result {
                Err(ExecutionError::Failure(ref hash)) if *hash == tx_hash => true,
                _ => false,
            },
            "expected transaction failure with hash {} but got {:?}",
            tx_hash,
            result
        );
        transport.assert_request("eth_sendRawTransaction", &[json!(tx_raw)]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request("eth_getTransactionReceipt", &[json!(tx_hash)]);
        transport.assert_no_more_requests();
    }
}
