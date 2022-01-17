//! Implementation of gas price estimation.

use primitive_types::U256;
use web3::types::U64;
use gas_estimation::EstimatedGasPrice;

#[derive(Debug, Default, PartialEq)]
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
        (U256::from_f64_lossy(value.0), U256::from_f64_lossy(value.1)).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_for_transaction_legacy() {
        //assert data for legacy type of transaction is prepared
        let resolved_gas_price = GasPrice::Legacy(100.into()).resolve_for_transaction();
        assert_eq!(resolved_gas_price.gas_price, Some(100.into()));
        assert_eq!(resolved_gas_price.transaction_type, None);
    }

    #[test]
    fn resolve_for_transaction_eip1559() {
        //assert data for eip1559 type of transaction is prepared
        let resolved_gas_price = GasPrice::Eip1559 {
            max_fee_per_gas: 100.into(),
            max_priority_fee_per_gas: 50.into(),
        }
        .resolve_for_transaction();
        assert_eq!(resolved_gas_price.max_fee_per_gas, Some(100.into()));
        assert_eq!(resolved_gas_price.max_priority_fee_per_gas, Some(50.into()));
        assert_eq!(resolved_gas_price.transaction_type, Some(2.into()));
    }

    #[test]
    fn gas_price_convertor_u256() {
        //assert that legacy type of transaction is built when single U256 value is provided
        let legacy_transaction_type: GasPrice = U256::from(100).into();
        assert_eq!(legacy_transaction_type, GasPrice::Legacy(100.into()));

        //assert that legacy type of transaction is built when single f64 value is provided
        let legacy_transaction_type: GasPrice = 100.0.into();
        assert_eq!(legacy_transaction_type, GasPrice::Legacy(100.into()));
    }

    #[test]
    fn gas_price_convertor_u256_u256() {
        //assert that EIP1559 type of transaction is built when double U256 value is provided
        let eip1559_transaction_type: GasPrice = (U256::from(100), U256::from(50)).into();
        assert_eq!(
            eip1559_transaction_type,
            GasPrice::Eip1559 {
                max_fee_per_gas: 100.into(),
                max_priority_fee_per_gas: 50.into()
            }
        );

        //assert that EIP1559 type of transaction is built when double f64 value is provided
        let eip1559_transaction_type: GasPrice = (100.0, 50.0).into();
        assert_eq!(
            eip1559_transaction_type,
            GasPrice::Eip1559 {
                max_fee_per_gas: 100.into(),
                max_priority_fee_per_gas: 50.into()
            }
        );
    }
}
