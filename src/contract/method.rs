//! Implementation for a contract method builder and call future. This is not
//! intended to be used directly but to be used by a contract `Instance` with
//! [Instance::method](ethcontract::contract::Instance::method).

use crate::errors::ExecutionError;
use crate::future::CompatQueryResult;
use crate::transaction::{Account, SendFuture, SendAndConfirmFuture, TransactionBuilder};
use ethabi::Function;
use futures::compat::Future01CompatExt;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use web3::api::Web3;
use web3::contract::tokens::Detokenize;
use web3::contract::QueryResult;
use web3::types::{Address, BlockNumber, Bytes, CallRequest, U256};
use web3::Transport;

/// Data used for building a contract method call or transaction. The method
/// builder can be demoted into a `CallBuilder` to not allow sending of
/// transactions. This is useful when dealing with view functions.
#[derive(Debug, Clone)]
pub struct MethodBuilder<T: Transport, R: Detokenize> {
	web3: Web3<T>,
	function: Function,
    /// transaction parameters
    pub tx: TransactionBuilder<T>,
    _result: PhantomData<R>,
}

impl<T: Transport, R: Detokenize> MethodBuilder<T, R> {
    /// Creates a new builder for a transaction.
    pub fn new(web3: Web3<T>, function: Function, address: Address, data: Bytes) -> MethodBuilder<T, R> {
        MethodBuilder {
        	web3: web3.clone(),
        	function,
        	tx: TransactionBuilder::new(web3).to(address).data(data),
        	_result: PhantomData,
        }
    }

    /// Specify the signing method to use for the transaction, if not specified
    /// the the transaction will be locally signed with the default user.
    pub fn from(mut self, value: Account) -> MethodBuilder<T, R> {
        self.tx = self.tx.from(value);
		self
    }

    /// Secify amount of gas to use, if not specified then a gas estimate will
    /// be used.
    pub fn gas(mut self, value: U256) -> MethodBuilder<T, R> {
        self.tx = self.tx.gas(value);
		self
    }

    /// Specify the gas price to use, if not specified then the estimated gas
    /// price will be used.
    pub fn gas_price(mut self, value: U256) -> MethodBuilder<T, R> {
        self.tx = self.tx.gas(value);
		self
    }

    /// Specify what how much ETH to transfer with the transaction, if not
    /// specified then no ETH will be sent.
    pub fn value(mut self, value: U256) -> MethodBuilder<T, R> {
        self.tx = self.tx.gas(value);
		self
    }

    /// Specify the nonce for the transation, if not specified will use the
    /// current transaction count for the signing account.
    pub fn nonce(mut self, value: U256) -> MethodBuilder<T, R> {
        self.tx = self.tx.gas(value);
		self
    }

    /// Extract inner `TransactionBuilder` from this `SendBuilder`. This exposes
    /// `TransactionBuilder` only APIs such as `estimate_gas`.
    pub fn into_inner(self) -> TransactionBuilder<T> {
        self.tx
    }

    /// Call a contract method. Contract calls do not modify the blockchain and
    /// as such do not require gas or signing.
    pub fn call(self) -> CallFuture<T, R> {
        self.call_with_block(None)
    }

    /// Call a contract method to be evaluated for a optionally specified block
    /// number. Contract calls do not modify the blockchain and as such do not
    /// require gas or signing.
    pub fn call_with_block(self, block: Option<BlockNumber>) -> CallFuture<T, R> {
        CallFuture::from_builder_with_block(self, block)
    }

    /// Sign (if required) and send the transaction.
    pub fn send(self) -> SendFuture<T> {
        self.tx.send()
    }

    /// Sign (if required) and send the transaction and then wait for
    /// confirmations.
   	pub fn send_and_confirm(
        self,
        poll_interval: Duration,
        confirmations: usize,
    ) -> SendAndConfirmFuture<T> {
        self.tx.send_and_confirm(poll_interval, confirmations)
    }
}

/// Future representing a pending contract call (i.e. query) to be resolved when
/// the call completes.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct CallFuture<T: Transport, R: Detokenize>(CompatQueryResult<T, R>);

impl<T: Transport, R: Detokenize> CallFuture<T, R> {
    /// Construct a new `CallFuture` from a `CallBuilder`.
    fn from_builder_with_block(builder: MethodBuilder<T, R>, block: Option<BlockNumber>) -> CallFuture<T, R> {
        CallFuture(
            QueryResult::new(
                builder.web3.eth().call(
                    CallRequest {
                        from: builder.tx.from.map(|account| account.address()),
                        to: builder.tx.to.unwrap_or_default(),
                        gas: builder.tx.gas,
                        gas_price: builder.tx.gas_price,
                        value: builder.tx.value,
                        data: builder.tx.data,
                    },
                    block,
                ),
                builder.function,
            )
            .compat(),
        )
    }

    /// Get a pinned reference to the inner `QueryResult` web3 future taht is
    /// actually driving the query.
    fn inner(self: Pin<&mut Self>) -> Pin<&mut CompatQueryResult<T, R>> {
        Pin::new(&mut self.get_mut().0)
    }
}

impl<T: Transport, R: Detokenize> Future for CallFuture<T, R> {
    type Output = Result<R, ExecutionError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.inner()
            .poll(cx)
            .map(|result| result.map_err(ExecutionError::from))
    }
}
