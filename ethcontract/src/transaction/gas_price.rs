//! Implementation of gas price estimation.

use primitive_types::U256;
use web3::types::U64;

#[derive(Debug, Default)]
/// Data related to gas price, prepared for populating the transaction object.
pub struct ResolvedTransactionGasPrice {
    /// Legacy gas price, populated if transaction type is legacy
    pub gas_price: Option<U256>,
    /// Maximum gas price willing to pay for the transaction, populated if transaction type is eip1559
    pub max_fee_per_gas: Option<U256>,
    /// Priority fee used to incentivize miners to include the tx in case of network congestion.
    /// Populated if transaction type is eip1559
    pub max_priority_fee_per_gas: Option<U256>,
    /// Equal to None for legacy transaction, equal to 2 for eip1559 transaction
    pub transaction_type: Option<U64>,
}

/// The gas price setting to use.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GasPrice {
    /// Legacy type of transactions, using single gas price value. Equivalent to sending
    /// eip1559 transaction with max_fee_per_gas = max_priority_fee_per_gas = gas_price
    Legacy(U256),

    /// Eip1559 type of transactions, using two values (max_fee_per_gas, max_priority_fee_per_gas)
    Eip1559 {
        /// Maximum gas price willing to pay for the transaction.
        max_fee_per_gas: U256,
        /// Priority fee used to incentivize miners to include the tx in case of network congestion.
        max_priority_fee_per_gas: U256,
    },
}

impl GasPrice {
    /// Prepares the data for transaction.
    pub fn resolve_for_transaction(&self) -> ResolvedTransactionGasPrice {
        match self {
            GasPrice::Legacy(value) => ResolvedTransactionGasPrice {
                gas_price: Some(*value),
                ..Default::default()
            },
            GasPrice::Eip1559 {
                max_fee_per_gas,
                max_priority_fee_per_gas,
            } => ResolvedTransactionGasPrice {
                max_fee_per_gas: Some(*max_fee_per_gas),
                max_priority_fee_per_gas: Some(*max_priority_fee_per_gas),
                transaction_type: Some(2.into()),
                ..Default::default()
            },
        }
    }
}

impl From<U256> for GasPrice {
    fn from(value: U256) -> Self {
        GasPrice::Legacy(value)
    }
}

impl From<f64> for GasPrice {
    fn from(value: f64) -> Self {
        U256::from_f64_lossy(value).into()
    }
}

impl From<(U256, U256)> for GasPrice {
    fn from(value: (U256, U256)) -> Self {
        GasPrice::Eip1559 {
            max_fee_per_gas: value.0,
            max_priority_fee_per_gas: value.1,
        }
    }
}

impl From<(f64, f64)> for GasPrice {
    fn from(value: (f64, f64)) -> Self {
        (U256::from_f64_lossy(value.0), U256::from_f64_lossy(value.0)).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::prelude::*;

    #[test]
    fn gas_price_scalling() {
        assert_eq!(scale_gas_price(1_000_000.into(), 2.0), 2_000_000.into());
        assert_eq!(scale_gas_price(1_000_000.into(), 1.5), 1_500_000.into());
        assert_eq!(scale_gas_price(U256::MAX, 2.0), U256::MAX);
    }

    #[test]
    fn resolve_gas_price() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let gas_price = U256::from(1_000_000);

        transport.add_response(json!(gas_price));
        assert_eq!(
            GasPrice::Standard
                .resolve(&web3)
                .immediate()
                .expect("error resolving gas price"),
            gas_price
        );
        transport.assert_request("eth_gasPrice", &[]);
        transport.assert_no_more_requests();

        transport.add_response(json!(gas_price));
        assert_eq!(
            GasPrice::Scaled(2.0)
                .resolve(&web3)
                .immediate()
                .expect("error resolving gas price"),
            gas_price * 2
        );
        transport.assert_request("eth_gasPrice", &[]);
        transport.assert_no_more_requests();

        assert_eq!(
            GasPrice::Value(gas_price)
                .resolve(&web3)
                .immediate()
                .expect("error resolving gas price"),
            gas_price
        );
        transport.assert_no_more_requests();
    }

    #[test]
    fn resolve_gas_price_for_transaction_request() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let gas_price = U256::from(1_000_000);

        assert_eq!(
            GasPrice::Standard
                .resolve_for_transaction_request(&web3)
                .immediate()
                .expect("error resolving gas price"),
            None
        );
        transport.assert_no_more_requests();

        transport.add_response(json!(gas_price));
        assert_eq!(
            GasPrice::Scaled(2.0)
                .resolve_for_transaction_request(&web3)
                .immediate()
                .expect("error resolving gas price"),
            Some(gas_price * 2),
        );
        transport.assert_request("eth_gasPrice", &[]);
        transport.assert_no_more_requests();

        assert_eq!(
            GasPrice::Value(gas_price)
                .resolve_for_transaction_request(&web3)
                .immediate()
                .expect("error resolving gas price"),
            Some(gas_price)
        );
        transport.assert_no_more_requests();
    }
}
