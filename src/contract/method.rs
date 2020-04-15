//! Implementation for a contract method builder and call future. This is not
//! intended to be used directly but to be used by a contract `Instance` with
//! [Instance::method](ethcontract::contract::Instance::method).

use crate::abicompat::AbiCompat;
use crate::errors::{revert, ExecutionError, MethodError};
use crate::future::CompatCallFuture;
use crate::transaction::send::SendFuture;
use crate::transaction::{Account, GasPrice, TransactionBuilder};
use ethcontract_common::abi::{Function, Token};
use futures::compat::Future01CompatExt;
use pin_project::pin_project;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use web3::api::Web3;
use web3::contract::tokens::Detokenize;
use web3::contract::Error as Web3ContractError;
use web3::types::{Address, BlockNumber, Bytes, CallRequest, U256};
use web3::Transport;

/// A void type to represent methods with empty return types.
///
/// This is used to work around the fact that `(): !Detokenize`.
pub struct Void(());

/// Represents a type can detokenize a result.
pub trait Detokenizable {
    /// The output that this type detokenizes into.
    type Output;

    /// Returns true if this is an empty type.
    fn is_empty() -> bool;

    /// Create an instance of `Output` by decoding tokens.
    fn from_tokens(tokens: Vec<Token>) -> Result<Self::Output, ExecutionError>;
}

impl Detokenizable for Void {
    type Output = ();

    fn is_empty() -> bool {
        true
    }

    fn from_tokens(tokens: Vec<Token>) -> Result<Self::Output, ExecutionError> {
        if !tokens.is_empty() {
            return Err(Web3ContractError::InvalidOutputType(format!(
                "Expected no elements, got tokens: {:?}",
                tokens
            ))
            .into());
        }

        Ok(())
    }
}

impl<T: Detokenize> Detokenizable for T {
    type Output = Self;

    fn is_empty() -> bool {
        false
    }

    fn from_tokens(tokens: Vec<Token>) -> Result<Self::Output, ExecutionError> {
        let tokens = match tokens.compat() {
            Some(tokens) => tokens,
            None => return Err(ExecutionError::UnsupportedToken),
        };
        let result = <T as Detokenize>::from_tokens(tokens)?;
        Ok(result)
    }
}

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
pub struct MethodBuilder<T: Transport, R: Detokenizable> {
    web3: Web3<T>,
    function: Function,
    /// transaction parameters
    pub tx: TransactionBuilder<T>,
    _result: PhantomData<R>,
}

impl<T: Transport> MethodBuilder<T, Void> {
    /// Creates a new builder for a transaction invoking the fallback method.
    pub fn fallback(web3: Web3<T>, address: Address, data: Bytes) -> Self {
        // NOTE: We create a fake `Function` entry for the fallback method. This
        //   is OK since it is only ever used for error formatting purposes.
        let function = Function {
            name: "fallback".into(),
            inputs: vec![],
            outputs: vec![],
            constant: false,
        };
        MethodBuilder::new(web3, function, address, data)
    }
}

impl<T: Transport, R: Detokenizable> MethodBuilder<T, R> {
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
    pub fn send(self) -> MethodSendFuture<T> {
        MethodFuture::new(self.function, self.tx.send())
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
    pub fn call(self) -> CallFuture<T, R> {
        self.view().call()
    }
}

/// Future that wraps an inner transaction execution future to add method
/// information to the error.
#[pin_project]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct MethodFuture<F> {
    function: Function,
    #[pin]
    inner: F,
}

impl<F> MethodFuture<F> {
    /// Creates a new `MethodFuture` from a function ABI declaration and an
    /// inner future.
    fn new(function: Function, inner: F) -> Self {
        MethodFuture { function, inner }
    }
}

impl<T, F> Future for MethodFuture<F>
where
    F: Future<Output = Result<T, ExecutionError>>,
{
    type Output = Result<T, MethodError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut this = self.project();
        this.inner
            .as_mut()
            .poll(cx)
            .map(|result| result.map_err(|err| MethodError::new(&this.function, err)))
    }
}

/// A type alias for a `MethodFuture` wrapped `SendFuture`.
pub type MethodSendFuture<T> = MethodFuture<SendFuture<T>>;

/// Data used for building a contract method call. The view method builder can't
/// directly send transactions and is for read only method calls.
#[derive(Debug, Clone)]
#[must_use = "view methods do nothing unless you `.call()` them"]
pub struct ViewMethodBuilder<T: Transport, R: Detokenizable> {
    /// method parameters
    pub m: MethodBuilder<T, R>,
    /// optional block number
    pub block: Option<BlockNumber>,
}

impl<T: Transport, R: Detokenizable> ViewMethodBuilder<T, R> {
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
    pub fn block(mut self, value: BlockNumber) -> Self {
        self.block = Some(value);
        self
    }
}

impl<T: Transport, R: Detokenizable> ViewMethodBuilder<T, R> {
    /// Call a contract method. Contract calls do not modify the blockchain and
    /// as such do not require gas or signing.
    pub fn call(self) -> CallFuture<T, R> {
        CallFuture::from_builder(self)
    }
}

/// Future representing a pending contract call (i.e. query) to be resolved when
/// the call completes.
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[pin_project]
pub struct CallFuture<T: Transport, R: Detokenizable> {
    function: Function,
    #[pin]
    call: CompatCallFuture<T, Bytes>,
    _result: PhantomData<Box<R>>,
}

impl<T: Transport, R: Detokenizable> CallFuture<T, R> {
    /// Construct a new `CallFuture` from a `ViewMethodBuilder`.
    fn from_builder(builder: ViewMethodBuilder<T, R>) -> Self {
        CallFuture {
            function: builder.m.function,
            call: builder
                .m
                .web3
                .eth()
                .call(
                    CallRequest {
                        from: builder.m.tx.from.map(|account| account.address()),
                        to: builder.m.tx.to.unwrap_or_default(),
                        gas: builder.m.tx.gas,
                        gas_price: builder
                            .m
                            .tx
                            .gas_price
                            .and_then(|gas_price| gas_price.value()),
                        value: builder.m.tx.value,
                        data: builder.m.tx.data,
                    },
                    builder.block,
                )
                .compat(),
            _result: PhantomData,
        }
    }
}

impl<T: Transport, R: Detokenizable> Future for CallFuture<T, R> {
    type Output = Result<R::Output, MethodError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut this = self.project();
        this.call.as_mut().poll(cx).map(|result| {
            result
                .map_err(ExecutionError::from)
                .and_then(|bytes| decode_geth_call_result::<R>(&this.function, bytes.0))
                .map_err(|err| MethodError::new(&this.function, err))
        })
    }
}

/// Decodes the raw bytes result from an `eth_call` request to check for reverts
/// and encoded revert messages.
///
/// This is required since Geth returns a success result from an `eth_call` that
/// reverts (or if an invalid opcode is executed) while other nodes like Ganache
/// encode this information in a JSON RPC error. On a revert or invalid opcode,
/// the result is `0x` (empty data), while on a revert with message, it is an
/// ABI encoded `Error(string)` function call data.
fn decode_geth_call_result<R: Detokenizable>(
    function: &Function,
    bytes: Vec<u8>,
) -> Result<R::Output, ExecutionError> {
    if let Some(reason) = revert::decode_reason(&bytes) {
        // This is an encoded revert message from Geth nodes.
        Err(ExecutionError::Revert(Some(reason)))
    } else if bytes.is_empty() && !R::is_empty() {
        // Geth does this on `revert()` without a message and `invalid()`,
        // just treat them all as `invalid()` as generally contracts revert
        // with messages.
        Err(ExecutionError::InvalidOpcode)
    } else {
        // just a plain ol' regular result, try and decode it
        let tokens = function.decode_output(&bytes)?;
        let result = R::from_tokens(tokens)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::prelude::*;
    use ethcontract_common::abi::{Param, ParamType};

    fn test_abi_function() -> (Function, Bytes) {
        let function = Function {
            name: "test".to_owned(),
            inputs: Vec::new(),
            outputs: vec![Param {
                name: "".to_owned(),
                kind: ParamType::Uint(256),
            }],
            constant: false,
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
        .block(BlockNumber::Number(100.into()));

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

    #[test]
    fn method_call_geth_revert_with_message() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let address = addr!("0x0123456789012345678901234567890123456789");
        let (function, data) = test_abi_function();
        let tx = ViewMethodBuilder::<_, U256>::from_method(MethodBuilder::new(
            web3, function, address, data,
        ));

        transport.add_response(json!(revert::encode_reason_hex("message"))); // call response
        let result = tx.call().immediate();
        assert!(
            match &result {
                Err(MethodError {
                    inner: ExecutionError::Revert(Some(ref reason)),
                    ..
                }) if reason == "message" => true,
                _ => false,
            },
            "unexpected result {:?}",
            result
        );
    }

    #[test]
    fn method_call_geth_revert() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let address = addr!("0x0123456789012345678901234567890123456789");
        let (function, data) = test_abi_function();
        let tx = ViewMethodBuilder::<_, U256>::from_method(MethodBuilder::new(
            web3, function, address, data,
        ));

        transport.add_response(json!("0x"));
        let result = tx.call().immediate();
        assert!(
            match &result {
                Err(MethodError {
                    inner: ExecutionError::InvalidOpcode,
                    ..
                }) => true,
                _ => false,
            },
            "unexpected result {:?}",
            result
        );
    }
}
