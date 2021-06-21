//! Implementation for creating instances for deployed contracts and deploying
//! new contracts.

use crate::errors::{DeployError, ExecutionError};
use crate::tokens::Tokenize;
use crate::transaction::{Account, GasPrice, TransactionBuilder, TransactionResult};
use ethcontract_common::abi::Error as AbiError;
use ethcontract_common::{Abi, Bytecode};
use std::marker::PhantomData;
use web3::api::Web3;
use web3::types::{Address, Bytes, H256, U256};
use web3::Transport;

/// a factory trait for deployable contract instances. this traits provides
/// functionality for building a deployment and creating instances of a
/// contract type at a given address.
///
/// this allows generated contracts to be deployable without having to create
/// new builder and future types.
pub trait Deploy<T: Transport>: Sized {
    /// The type of the contract instance being created.
    type Context;

    /// Gets a reference to the contract bytecode.
    fn bytecode(cx: &Self::Context) -> &Bytecode;

    /// Gets a reference the contract ABI.
    fn abi(cx: &Self::Context) -> &Abi;

    /// Create a contract instance from the specified deployment.
    fn from_deployment(
        web3: Web3<T>,
        address: Address,
        transaction_hash: H256,
        cx: Self::Context,
    ) -> Self;
}

/// Builder for specifying options for deploying a linked contract.
#[derive(Debug, Clone)]
#[must_use = "deploy builers do nothing unless you `.deploy()` them"]
pub struct DeployBuilder<T, I>
where
    T: Transport,
    I: Deploy<T>,
{
    /// The underlying `web3` provider.
    web3: Web3<T>,
    /// The factory context.
    context: I::Context,
    /// The underlying transaction used t
    tx: TransactionBuilder<T>,
    _instance: PhantomData<I>,
}

impl<T, I> DeployBuilder<T, I>
where
    T: Transport,
    I: Deploy<T>,
{
    /// Create a new deploy builder from a `web3` provider, contract data and
    /// deployment (constructor) parameters.
    pub fn new<P>(web3: Web3<T>, context: I::Context, params: P) -> Result<Self, DeployError>
    where
        P: Tokenize,
    {
        // NOTE(nlordell): unfortunately here we have to re-implement some
        //   `rust-web3` code so that we can add things like signing support;
        //   luckily most of complicated bits can be reused from the tx code

        let bytecode = I::bytecode(&context);
        if bytecode.is_empty() {
            return Err(DeployError::EmptyBytecode);
        }

        let code = bytecode.to_bytes()?;
        let params = match params.into_token() {
            ethcontract_common::abi::Token::Tuple(tokens) => tokens,
            _ => unreachable!("function arguments are always tuples"),
        };
        let data = match (I::abi(&context).constructor(), params.is_empty()) {
            (None, false) => return Err(AbiError::InvalidData.into()),
            (None, true) => code,
            (Some(ctor), _) => Bytes(ctor.encode_input(code.0, &params)?),
        };

        Ok(DeployBuilder {
            web3: web3.clone(),
            context,
            tx: TransactionBuilder::new(web3).data(data).confirmations(0),
            _instance: PhantomData,
        })
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

    /// Extract inner `TransactionBuilder` from this `DeployBuilder`. This
    /// exposes `TransactionBuilder` only APIs.
    pub fn into_inner(self) -> TransactionBuilder<T> {
        self.tx
    }

    /// Sign (if required) and execute the transaction. Returns the transaction
    /// hash that can be used to retrieve transaction information.
    pub async fn deploy(self) -> Result<I, DeployError> {
        let tx = match self.tx.send().await? {
            TransactionResult::Receipt(tx) => tx,
            TransactionResult::Hash(tx) => return Err(DeployError::Pending(tx)),
        };

        let transaction_hash = tx.transaction_hash;
        let address = tx
            .contract_address
            .ok_or_else(|| ExecutionError::Failure(Box::new(tx)))?;

        Ok(I::from_deployment(
            self.web3,
            address,
            transaction_hash,
            self.context,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{Instance, Linker};
    use crate::test::prelude::*;
    use ethcontract_common::{Contract, Bytecode};

    type InstanceDeployBuilder<T> = DeployBuilder<T, Instance<T>>;

    #[test]
    fn deploy_tx_options() {
        let transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let from = addr!("0x9876543210987654321098765432109876543210");
        let bytecode = Bytecode::from_hex_str("0x42").unwrap();
        let contract = Contract {
            bytecode: bytecode.clone(),
            ..Contract::empty()
        };
        let linker = Linker::new(contract);
        let tx = InstanceDeployBuilder::new(web3, linker, ())
            .expect("error creating deploy builder")
            .from(Account::Local(from, None))
            .gas(1.into())
            .gas_price(2.into())
            .value(28.into())
            .nonce(42.into())
            .into_inner();

        assert_eq!(tx.from.map(|a| a.address()), Some(from));
        assert_eq!(tx.to, None);
        assert_eq!(tx.gas, Some(1.into()));
        assert_eq!(tx.gas_price, Some(2.into()));
        assert_eq!(tx.value, Some(28.into()));
        assert_eq!(tx.data, Some(bytecode.to_bytes().unwrap()));
        assert_eq!(tx.nonce, Some(42.into()));
        transport.assert_no_more_requests();
    }

    #[test]
    fn deploy() {
        // TODO(nlordell): implement this test - there is an open issue for this
        //   on github
    }

    #[test]
    fn deploy_fails_on_empty_bytecode() {
        let transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let contract = Contract::empty();
        let linker = Linker::new(contract);
        let error = InstanceDeployBuilder::new(web3, linker, ()).err().unwrap();

        assert_eq!(error.to_string(), DeployError::EmptyBytecode.to_string());
        transport.assert_no_more_requests();
    }
}
