//! Implementation of gas price estimation.

use primitive_types::U256;
use web3::types::U64;

/// The gas price setting to use.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GasPrice {
    /// Legacy type of transactions, using single gas price value. Equivalent to sending
    /// eip1559 transaction with max_fee_per_gas = max_priority_fee_per_gas = gas_price
    Legacy(U256),

    /// Eip1559 type of transactions, using two values (max_fee_per_gas, max_priority_fee_per_gas)
    Eip1559((U256, U256)),
}

impl GasPrice {
    /// Prepares the data for transaction. Returns tuple:
    /// (gas_price, max_fee_per_gas, max_priority_fee_per_gas, transaction_type)
    pub fn resolve_for_transaction(
        &self,
    ) -> (Option<U256>, Option<U256>, Option<U256>, Option<U64>) {
        match self {
            GasPrice::Legacy(value) => (Some(*value), None, None, None),
            GasPrice::Eip1559(pair) => (None, Some(pair.0), Some(pair.1), Some(2.into())),
        }
    }
}

impl Default for GasPrice {
    fn default() -> Self {
        GasPrice::Eip1559(Default::default())
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
        GasPrice::Eip1559(value)
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
