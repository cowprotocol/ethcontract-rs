//! Implementation for setting up, signing, estimating gas and sending
//! transactions on the Ethereum network.

pub mod build;
pub mod confirm;
pub mod gas_price;
pub mod estimate_gas;
pub mod send;

use crate::transaction::build::BuildFuture;
use crate::transaction::confirm::ConfirmParams;
use crate::transaction::estimate_gas::EstimateGasFuture;
use crate::transaction::send::SendFuture;
use ethsign::{Protected, SecretKey};
use web3::api::Web3;
use web3::types::{
    Address, Bytes, TransactionCondition, TransactionReceipt, TransactionRequest, H256, U256,
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
    /// price.
    pub gas_price: Option<U256>,
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
    pub fn gas_price(mut self, value: U256) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::ExecutionError;
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
    fn tx_build_local_default_account() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let accounts = [
            addr!("0x9876543210987654321098765432109876543210"),
            addr!("0x1111111111111111111111111111111111111111"),
            addr!("0x2222222222222222222222222222222222222222"),
        ];

        transport.add_response(json!(accounts)); // get accounts
        let tx = TransactionBuilder::new(web3)
            .build()
            .immediate()
            .expect("get accounts success")
            .request()
            .expect("transaction request");

        transport.assert_request("eth_accounts", &[]);
        transport.assert_no_more_requests();

        // assert that if no from is specified that it uses the first account
        assert_eq!(tx.from, accounts[0]);
    }

    #[test]
    fn tx_build_locked() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let from = addr!("0x9876543210987654321098765432109876543210");
        let pw = "foobar";
        let to = addr!("0x0000000000000000000000000000000000000000");
        let signed = bytes!("0x0123456789"); // doesn't have to be valid, we don't check

        transport.add_response(json!({
            "raw": signed,
            "tx": {
                "hash": "0x0000000000000000000000000000000000000000000000000000000000000000",
                "nonce": "0x0",
                "from": from,
                "value": "0x0",
                "gas": "0x0",
                "gasPrice": "0x0",
                "input": "0x",
            }
        })); // sign transaction
        let tx = TransactionBuilder::new(web3)
            .from(Account::Locked(from, pw.into(), None))
            .to(to)
            .build()
            .immediate()
            .expect("sign succeeded")
            .raw()
            .expect("raw transaction");

        transport.assert_request(
            "personal_signTransaction",
            &[
                json!({
                    "from": from,
                    "to": to,
                }),
                json!(pw),
            ],
        );
        transport.assert_no_more_requests();

        assert_eq!(tx, signed);
    }

    #[test]
    fn tx_build_offline() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let key = key!("0x0102030405060708091011121314151617181920212223242526272829303132");
        let from: Address = key.public().address().into();
        let to = addr!("0x0000000000000000000000000000000000000000");

        let gas = uint!("0x9a5");
        let gas_price = uint!("0x1ce");
        let nonce = uint!("0x42");
        let chain_id = 77777;

        transport.add_response(json!(gas));
        transport.add_response(json!(gas_price));
        transport.add_response(json!(nonce));
        transport.add_response(json!(format!("{:#x}", chain_id)));

        let tx1 = TransactionBuilder::new(web3.clone())
            .from(Account::Offline(key.clone(), None))
            .to(to)
            .build()
            .immediate()
            .expect("sign succeeded")
            .raw()
            .expect("raw transaction");

        // assert that we ask the node for all the missing values
        transport.assert_request(
            "eth_estimateGas",
            &[json!({
                "from": from,
                "to": to,
            })],
        );
        transport.assert_request("eth_gasPrice", &[]);
        transport.assert_request("eth_getTransactionCount", &[json!(from), json!("latest")]);
        transport.assert_request("eth_chainId", &[]);
        transport.assert_no_more_requests();

        let tx2 = TransactionBuilder::new(web3)
            .from(Account::Offline(key, Some(chain_id)))
            .to(to)
            .gas(gas)
            .gas_price(gas_price)
            .nonce(nonce)
            .build()
            .immediate()
            .expect("sign succeeded")
            .raw()
            .expect("raw transaction");

        // assert that if we provide all the values then we can sign right away
        transport.assert_no_more_requests();

        // check that if we sign with same values we get same results
        assert_eq!(tx1, tx2);
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
