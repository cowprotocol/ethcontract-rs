//! Implementation of `eth_estimateGas` with workaround for Geth (Infura)
//! incompatabilities.

use crate::errors::ExecutionError;
use crate::transaction::TransactionBuilder;
use futures::compat::Future01CompatExt;
use serde::Serialize;
use web3::api::Web3;
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

/// Extention trait for implementing gas estimation for transactions that allows
/// estimating gas on contract deployments.
pub async fn estimate_gas<T: Transport>(
    web3: &Web3<T>,
    request: EstimateGasRequest,
) -> Result<U256, ExecutionError> {
    let gas = CallFuture::new(
        web3.transport()
            .execute("eth_estimateGas", vec![helpers::serialize(&request)]),
    )
    .compat()
    .await?;

    Ok(gas)
}

impl<T: Transport> TransactionBuilder<T> {
    /// Estimate the gas required for this transaction.
    pub async fn estimate_gas(self) -> Result<U256, ExecutionError> {
        let from = self.from.map(|account| account.address());
        let gas_price = self.gas_price.and_then(|gas_price| gas_price.value());

        estimate_gas(
            &self.web3,
            EstimateGasRequest {
                from,
                to: self.to,
                gas: None,
                gas_price,
                value: self.value,
                data: self.data,
            },
        )
        .await
    }
}
