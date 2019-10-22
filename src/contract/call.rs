use crate::contract::errors::ExecutionError;
use ethabi::Function;
use futures::compat::{Compat01As03,Future01CompatExt};
use std::task::{Poll, Context};
use std::future::Future;
use std::pin::Pin;
use std::marker::PhantomData;
use web3::api::Web3;
use web3::types::{Address, CallRequest, BlockNumber, Bytes};
use web3::contract::tokens::Detokenize;
use web3::contract::QueryResult;
use web3::Transport;

/// Data used for building a contract call (i.e. query). Contract calls do not
/// modify the blockchain and as such do not require gas, signing and cannot
/// accept value.
#[derive(Clone, Debug)]
pub struct CallBuilder<T: Transport, R: Detokenize> {
    web3: Web3<T>,
    function: Function,
    address: Address,
    data: Bytes,
    /// optional from address
    pub from: Option<Address>,
    /// optional block number
    pub block: Option<BlockNumber>,
    _result: PhantomData<R>,
}

impl<T: Transport, R: Detokenize> CallBuilder<T, R> {
    /// Create a new builder for a contract call.
    pub fn new(web3: Web3<T>, function: Function, address: Address, data: Bytes) -> CallBuilder<T, R> {
        CallBuilder {
            web3,
            function,
            address,
            data,
            from: None,
            block: None,
            _result: PhantomData,
        }
    }
    
    /// Specify from address for the contract call.
    pub fn from(mut self, address: Address) -> CallBuilder<T, R> {
        self.from = Some(address);
        self
    }

    /// Specify block number to use for the contract call.
    pub fn block(mut self, n: BlockNumber) -> CallBuilder<T, R> {
        self.block = Some(n);
        self
    }

    /// Execute the call to the contract and return the data
    pub fn execute(self) -> ExecuteCallFuture<T, R> {
        ExecuteCallFuture::from_builder(self)
    }
}

/// Future representing a pending contract call (i.e. query) to be resolved when
/// the call completes.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct ExecuteCallFuture<T: Transport, R: Detokenize>(Compat01As03<QueryResult<R, T::Out>>);

impl<T: Transport, R: Detokenize> ExecuteCallFuture<T, R> {
    /// Construct a new `ExecuteCallFuture` from a `CallBuilder`.
    fn from_builder(builder: CallBuilder<T, R>) -> ExecuteCallFuture<T, R> {
        ExecuteCallFuture(
            QueryResult::new(
                builder.web3.eth().call(
                    CallRequest {
                        from: builder.from,
                        to: builder.address,
                        gas: None,
                        gas_price: None,
                        value: None,
                        data: Some(builder.data),
                    },
                    builder.block,
                ),
                builder.function,
            )
            .compat(),
        )
    }

    /// Get a pinned reference to the inner `QueryResult` web3 future taht is
    /// actually driving the query.
    fn inner(self: Pin<&mut Self>) -> Pin<&mut Compat01As03<QueryResult<R, T::Out>>> {
        Pin::new(&mut self.get_mut().0)
    }
}

impl<T: Transport, R: Detokenize> Future for ExecuteCallFuture<T, R> {
    type Output = Result<R, ExecutionError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.inner()
            .poll(cx)
            .map(|result| result.map_err(ExecutionError::from))
    }
}
