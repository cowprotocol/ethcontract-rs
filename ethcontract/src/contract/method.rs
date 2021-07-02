//! Implementation for a contract method builder and call future. This is not
//! intended to be used directly but to be used by a contract `Instance` with
//! [Instance::method](ethcontract::contract::Instance::method).

use crate::transaction::{Account, GasPrice, TransactionBuilder, TransactionResult};
use crate::{batch::CallBatch, errors::MethodError, tokens::Tokenize};
use ethcontract_common::abi::{Function, Token};
use std::marker::PhantomData;
use web3::types::{Address, BlockId, Bytes, CallRequest, U256};
use web3::Transport;
use web3::{api::Web3, BatchTransport};

/// Default options to be applied to `MethodBuilder` or `ViewMethodBuilder`.
#[derive(Clone, Debug, Default)]
pub struct MethodDefaults {
    /// Default sender of the transaction with the signing strategy to use.
    pub from: Option<Account>,
    /// Default gas amount to use for transaction.
    pub gas: Option<U256>,
    /// Default gas price to use for transaction.
    pub gas_price: Option<GasPrice>,
}

/// Data used for building a contract method call or transaction. The method
/// builder can be demoted into a `CallBuilder` to not allow sending of
/// transactions. This is useful when dealing with view functions.
#[derive(Debug, Clone)]
#[must_use = "methods do nothing unless you `.call()` or `.send()` them"]
pub struct MethodBuilder<T: Transport, R: Tokenize> {
    web3: Web3<T>,
    function: Function,
    /// transaction parameters
    pub tx: TransactionBuilder<T>,
    _result: PhantomData<R>,
}

impl<T: Transport> MethodBuilder<T, ()> {
    /// Creates a new builder for a transaction invoking the fallback method.
    pub fn fallback(web3: Web3<T>, address: Address, data: Bytes) -> Self {
        // NOTE: We create a fake `Function` entry for the fallback method. This
        //   is OK since it is only ever used for error formatting purposes.

        #[allow(deprecated)]
        let function = Function {
            name: "fallback".into(),
            inputs: vec![],
            outputs: vec![],
            constant: false,
            state_mutability: Default::default(),
        };
        MethodBuilder::new(web3, function, address, data)
    }
}

impl<T: Transport, R: Tokenize> MethodBuilder<T, R> {
    /// Creates a new builder for a transaction.
    pub fn new(web3: Web3<T>, function: Function, address: Address, data: Bytes) -> Self {
        MethodBuilder {
            web3: web3.clone(),
            function,
            tx: TransactionBuilder::new(web3).to(address).data(data),
            _result: PhantomData,
        }
    }

    /// Apply method defaults to this builder.
    pub fn with_defaults(mut self, defaults: &MethodDefaults) -> Self {
        self.tx.from = self.tx.from.or_else(|| defaults.from.clone());
        self.tx.gas = self.tx.gas.or(defaults.gas);
        self.tx.gas_price = self.tx.gas_price.or(defaults.gas_price);
        self
    }

    /// Specify the signing method to use for the transaction, if not specified
    /// the the transaction will be locally signed with the default user.
    pub fn from(mut self, value: Account) -> Self {
        self.tx = self.tx.from(value);
        self
    }

    /// Secify amount of gas to use, if not specified then a gas estimate will
    /// be used.
    pub fn gas(mut self, value: U256) -> Self {
        self.tx = self.tx.gas(value);
        self
    }

    /// Specify the gas price to use, if not specified then the estimated gas
    /// price will be used.
    pub fn gas_price(mut self, value: GasPrice) -> Self {
        self.tx = self.tx.gas_price(value);
        self
    }

    /// Specify what how much ETH to transfer with the transaction, if not
    /// specified then no ETH will be sent.
    pub fn value(mut self, value: U256) -> Self {
        self.tx = self.tx.value(value);
        self
    }

    /// Specify the nonce for the transation, if not specified will use the
    /// current transaction count for the signing account.
    pub fn nonce(mut self, value: U256) -> Self {
        self.tx = self.tx.nonce(value);
        self
    }

    /// Specify the number of confirmations to wait for when confirming the
    /// transaction, if not specified will wait for the transaction to be mined
    /// without any extra confirmations.
    pub fn confirmations(mut self, value: usize) -> Self {
        self.tx = self.tx.confirmations(value);
        self
    }

    /// Extract inner `TransactionBuilder` from this `SendBuilder`. This exposes
    /// `TransactionBuilder` only APIs.
    pub fn into_inner(self) -> TransactionBuilder<T> {
        self.tx
    }

    /// Sign (if required) and send the method call transaction.
    pub async fn send(self) -> Result<TransactionResult, MethodError> {
        let Self { function, tx, .. } = self;
        tx.send()
            .await
            .map_err(|err| MethodError::new(&function, err))
    }

    /// Demotes a `MethodBuilder` into a `ViewMethodBuilder` which has a more
    /// restricted API and cannot actually send transactions.
    pub fn view(self) -> ViewMethodBuilder<T, R> {
        ViewMethodBuilder::from_method(self)
    }

    /// Call a contract method. Contract calls do not modify the blockchain and
    /// as such do not require gas or signing. Note that doing a call with a
    /// block number requires first demoting the `MethodBuilder` into a
    /// `ViewMethodBuilder` and setting the block number for the call.
    pub async fn call(self) -> Result<R, MethodError> {
        self.view().call().await
    }
}

/// Data used for building a contract method call. The view method builder can't
/// directly send transactions and is for read only method calls.
#[derive(Debug, Clone)]
#[must_use = "view methods do nothing unless you `.call()` them"]
pub struct ViewMethodBuilder<T: Transport, R: Tokenize> {
    /// method parameters
    pub m: MethodBuilder<T, R>,
    /// optional block number
    pub block: Option<BlockId>,
}

impl<T: Transport, R: Tokenize> ViewMethodBuilder<T, R> {
    /// Create a new `ViewMethodBuilder` by demoting a `MethodBuilder`.
    pub fn from_method(method: MethodBuilder<T, R>) -> Self {
        ViewMethodBuilder {
            m: method,
            block: None,
        }
    }

    /// Apply method defaults to this builder.
    pub fn with_defaults(mut self, defaults: &MethodDefaults) -> Self {
        self.m = self.m.with_defaults(defaults);
        self
    }

    /// Specify the account the transaction is being sent from.
    pub fn from(mut self, value: Address) -> Self {
        self.m = self.m.from(Account::Local(value, None));
        self
    }

    /// Secify amount of gas to use, if not specified then a gas estimate will
    /// be used.
    pub fn gas(mut self, value: U256) -> Self {
        self.m = self.m.gas(value);
        self
    }

    /// Specify the gas price to use, if not specified then the estimated gas
    /// price will be used.
    pub fn gas_price(mut self, value: GasPrice) -> Self {
        self.m = self.m.gas_price(value);
        self
    }

    /// Specify what how much ETH to transfer with the transaction, if not
    /// specified then no ETH will be sent.
    pub fn value(mut self, value: U256) -> Self {
        self.m = self.m.value(value);
        self
    }

    /// Specify the nonce for the transation, if not specified will use the
    /// current transaction count for the signing account.
    pub fn block(mut self, value: BlockId) -> Self {
        self.block = Some(value);
        self
    }
}

impl<T: Transport, R: Tokenize> ViewMethodBuilder<T, R> {
    /// Call a contract method. Contract calls do not modify the blockchain and
    /// as such do not require gas or signing.
    pub async fn call(self) -> Result<R, MethodError> {
        let eth = &self.m.web3.eth();
        let (function, call, block) = self.decompose();
        let future = eth.call(call, block);
        convert_response::<_, R>(future, function).await
    }

    /// Adds this view method to a batch. Allows execution with other contract calls in one roundtrip
    /// The returned future only resolve once `batch` is resolved. Panics, if `batch` is dropped before
    /// executing
    pub fn batch_call<B: BatchTransport>(
        self,
        batch: &mut CallBatch<B>,
    ) -> impl std::future::Future<Output = Result<R, MethodError>> {
        let (function, call, block) = self.decompose();
        let future = batch.push(call, block);
        async move { convert_response::<_, R>(future, function).await }
    }

    fn decompose(self) -> (Function, CallRequest, Option<BlockId>) {
        (
            self.m.function,
            CallRequest {
                from: self.m.tx.from.map(|account| account.address()),
                to: Some(self.m.tx.to.unwrap_or_default()),
                gas: self.m.tx.gas,
                gas_price: self.m.tx.gas_price.and_then(|gas_price| gas_price.value()),
                value: self.m.tx.value,
                data: self.m.tx.data,
                transaction_type: None,
                access_list: None,
            },
            self.block,
        )
    }
}

async fn convert_response<
    F: std::future::Future<Output = Result<Bytes, web3::Error>>,
    R: Tokenize,
>(
    future: F,
    function: Function,
) -> Result<R, MethodError> {
    let bytes = future
        .await
        .map_err(|err| MethodError::new(&function, err))?;
    let tokens = function
        .decode_output(&bytes.0)
        .map_err(|err| MethodError::new(&function, err))?;
    let token = match tokens.len() {
        0 => Token::Tuple(Vec::new()),
        1 => tokens.into_iter().next().unwrap(),
        // Older versions of solc emit a list of tokens as the return type of functions returning
        // tuples instead of a single type that is a tuple. In order to be backwards compatible we
        // accept this too.
        _ => Token::Tuple(tokens),
    };
    let result = R::from_token(token).map_err(|err| MethodError::new(&function, err))?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::prelude::*;
    use ethcontract_common::abi::{Param, ParamType};

    fn test_abi_function() -> (Function, Bytes) {
        #[allow(deprecated)]
        let function = Function {
            name: "test".to_owned(),
            inputs: Vec::new(),
            outputs: vec![Param {
                name: "".to_owned(),
                kind: ParamType::Uint(256),
            }],
            constant: false,
            state_mutability: Default::default(),
        };
        let data = function
            .encode_input(&[])
            .expect("error encoding empty input");

        (function, Bytes(data))
    }

    #[test]
    fn method_tx_options() {
        let transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let address = addr!("0x0123456789012345678901234567890123456789");
        let from = addr!("0x9876543210987654321098765432109876543210");
        let (function, data) = test_abi_function();
        let tx = MethodBuilder::<_, U256>::new(web3, function, address, data.clone())
            .from(Account::Local(from, None))
            .gas(1.into())
            .gas_price(2.into())
            .value(28.into())
            .nonce(42.into())
            .into_inner();

        assert_eq!(tx.from.map(|a| a.address()), Some(from));
        assert_eq!(tx.to, Some(address));
        assert_eq!(tx.gas, Some(1.into()));
        assert_eq!(tx.gas_price, Some(2.into()));
        assert_eq!(tx.value, Some(28.into()));
        assert_eq!(tx.data, Some(data));
        assert_eq!(tx.nonce, Some(42.into()));
        transport.assert_no_more_requests();
    }

    #[test]
    fn view_method_call() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let address = addr!("0x0123456789012345678901234567890123456789");
        let from = addr!("0x9876543210987654321098765432109876543210");
        let (function, data) = test_abi_function();
        let tx = ViewMethodBuilder::<_, U256>::from_method(MethodBuilder::new(
            web3,
            function,
            address,
            data.clone(),
        ))
        .from(from)
        .gas(1.into())
        .gas_price(2.into())
        .value(28.into())
        .block(BlockId::Number(100.into()));

        transport.add_response(json!(
            "0x000000000000000000000000000000000000000000000000000000000000002a"
        )); // call response
        let result = tx.call().immediate().expect("call error");

        assert_eq!(result, 42.into());
        transport.assert_request(
            "eth_call",
            &[
                json!({
                    "from": from,
                    "to": address,
                    "gas": "0x1",
                    "gasPrice": "0x2",
                    "value": "0x1c",
                    "data": data,
                }),
                json!("0x64"),
            ],
        );
        transport.assert_no_more_requests();
    }

    #[test]
    fn method_to_view_method_preserves_options() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let address = addr!("0x0123456789012345678901234567890123456789");
        let (function, data) = test_abi_function();
        let tx = MethodBuilder::<_, U256>::new(web3, function, address, data.clone())
            .gas(42.into())
            .view();

        transport.add_response(json!(
            "0x0000000000000000000000000000000000000000000000000000000000000000"
        ));
        tx.call().immediate().expect("call error");

        transport.assert_request(
            "eth_call",
            &[
                json!({
                    "to": address,
                    "gas": "0x2a",
                    "data": data,
                }),
                json!("latest"),
            ],
        );
        transport.assert_no_more_requests();
    }

    #[test]
    fn method_defaults_are_applied() {
        let transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let from = addr!("0x9876543210987654321098765432109876543210");
        let address = addr!("0x0123456789012345678901234567890123456789");
        let (function, data) = test_abi_function();
        let tx = MethodBuilder::<_, U256>::new(web3, function, address, data)
            .with_defaults(&MethodDefaults {
                from: Some(Account::Local(from, None)),
                gas: Some(1.into()),
                gas_price: Some(2.into()),
            })
            .into_inner();

        assert_eq!(tx.from.map(|a| a.address()), Some(from));
        assert_eq!(tx.gas, Some(1.into()));
        assert_eq!(tx.gas_price, Some(2.into()));
        transport.assert_no_more_requests();
    }
}
