//! Implementation of `eth_estimateGas` with workaround for Geth (Infura)
//! incompatabilities.

use crate::errors::ExecutionError;
use crate::future::CompatCallFuture;
use crate::transaction::TransactionBuilder;
use futures::compat::Future01CompatExt;
use pin_project::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use web3::api::{Eth, Namespace};
use web3::helpers::{self, CallFuture};
use web3::types::{Address, CallRequest, U256};
use web3::Transport;

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

    /// Create an estimate gas future from a `web3` `CallRequest`.
    pub(crate) fn from_request(eth: Eth<T>, request: CallRequest) -> Self {
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

    /// Gets the inner `CallFuture`.
    pub(crate) fn into_inner(self) -> CompatCallFuture<T, U256> {
        self.0
    }
}

impl<T: Transport> Future for EstimateGasFuture<T> {
    type Output = Result<U256, ExecutionError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.project().0.poll(cx).map_err(ExecutionError::from)
    }
}
