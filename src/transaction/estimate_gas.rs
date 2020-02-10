//! Implementation of `eth_estimateGas` with workaround for Geth (Infura)
//! incompatabilities.

use crate::errors::ExecutionError;
use crate::future::CompatCallFuture;
use crate::transaction::TransactionBuilder;
use futures::compat::Future01CompatExt;
use pin_project::pin_project;
use serde::Serialize;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use web3::api::{Eth, Namespace};
use web3::helpers::{self, CallFuture};
use web3::types::{Address, Bytes, U256};
use web3::Transport;

/// Transaction parameters used for estimating gas.
///
/// Note that this is similar to `web3::types::CallRequest` with the notable
/// exception that it allows for `to` to be `None` for estimating gas on
/// contract deployments.
#[derive(Clone, Debug, Serialize)]
pub struct EstimateGasRequest {
    /// The address of the sender
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<Address>,
    /// The to address, use `None` for contract deployment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<Address>,
    /// The maximum gas supplied to the transaction when estimating gas or
    /// `None` for unlimited.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas: Option<U256>,
    /// The gas price for the transaction or `None` for the node's median gas
    /// price.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "gasPrice")]
    pub gas_price: Option<U256>,
    /// The transfered value in wei or `None` for no value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<U256>,
    /// The data or `None` for empty data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Bytes>,
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
        let gas_price = builder.gas_price.and_then(|gas_price| gas_price.value());
        let request = EstimateGasRequest {
            from,
            to: builder.to,
            gas: None,
            gas_price,
            value: builder.value,
            data: builder.data,
        };

        EstimateGasFuture::from_request(eth, request)
    }

    /// Create an estimate gas future from a `web3` `EstimateGasRequest`.
    pub(crate) fn from_request(eth: Eth<T>, request: EstimateGasRequest) -> Self {
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
