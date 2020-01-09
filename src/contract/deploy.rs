//! Implementation for creating instances for deployed contracts and deploying
//! new contracts.

use crate::errors::{DeployError, ExecutionError};
use crate::future::{CompatCallFuture, Web3Unpin};
use crate::transaction::{Account, SendFuture, TransactionBuilder, TransactionResult};
use ethcontract_common::abi::ErrorKind as AbiErrorKind;
use ethcontract_common::{Abi, Bytecode};
use futures::compat::Future01CompatExt;
use futures::ready;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use web3::api::Web3;
use web3::contract::tokens::Tokenize;
use web3::types::{Address, Bytes, U256};
use web3::Transport;

/// a factory trait for deployable contract instances. this traits provides
/// functionality for creating instances of a contract type for a given network
/// ID.
///
/// this allows generated contracts to be deployable without having to create
/// new builder and future types.
pub trait Deployments<T: Transport>: Unpin {
    /// The type of the contract instance being created.
    type Instance;

    /// Create a contract instance for the specified network. This method should
    /// return `None` when no deployment can be found for the specified network
    /// ID.
    fn from_network(self, web3: Web3<T>, network_id: &str) -> Option<Self::Instance>;
}

/// Future for creating a deployed contract instance.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct DeployedFuture<T, D>
where
    T: Transport,
    D: Deployments<T>,
{
    /// The deployment arguments.
    args: Option<(Web3Unpin<T>, D)>,
    /// The factory used to locate the contract address from a netowkr ID.
    /// Underlying future for retrieving the network ID.
    network_id: CompatCallFuture<T, String>,
}

impl<T, D> DeployedFuture<T, D>
where
    T: Transport,
    D: Deployments<T>,
{
    /// Construct a new future that resolves when a deployed contract is located
    /// from a `web3` provider and artifact data.
    pub fn new(web3: Web3<T>, deployments: D) -> Self {
        let net = web3.net();
        DeployedFuture {
            args: Some((web3.into(), deployments)),
            network_id: net.version().compat(),
        }
    }
}

impl<T, D> Future for DeployedFuture<T, D>
where
    T: Transport,
    D: Deployments<T>,
{
    type Output = Result<D::Instance, DeployError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let unpinned = self.get_mut();
        Pin::new(&mut unpinned.network_id)
            .poll(cx)
            .map(|network_id| {
                let network_id = network_id?;
                let (web3, deployments) = unpinned.args.take().expect("called more than once");
                deployments
                    .from_network(web3.into(), &network_id)
                    .ok_or(DeployError::NotFound(network_id))
            })
    }
}

/// a factory trait for deployable contract instances. this traits provides
/// functionality for building a deployment and creating instances of a
/// contract type at a given address.
///
/// this allows generated contracts to be deployable without having to create
/// new builder and future types.
pub trait Factory<T: Transport>: Unpin {
    /// The type of the contract instance being created.
    type Instance;

    /// Gets a reference to the contract bytecode.
    fn bytecode(&self) -> &Bytecode;

    /// Gets a reference the contract ABI.
    fn abi(&self) -> &Abi;

    /// Create a contract instance from the specified address.
    fn at_address(self, web3: Web3<T>, address: Address) -> Self::Instance;
}

/// Builder for specifying options for deploying a linked contract.
#[derive(Debug, Clone)]
#[must_use = "deploy builers do nothing unless you `.deploy()` them"]
pub struct DeployBuilder<T, F>
where
    T: Transport,
    F: Factory<T>,
{
    /// The underlying `web3` provider.
    web3: Web3<T>,
    /// The instance factory to use once the contract is deployed and its
    /// address is retrieved.
    factory: F,
    /// The underlying transaction used t
    tx: TransactionBuilder<T>,
}

impl<T, F> DeployBuilder<T, F>
where
    T: Transport,
    F: Factory<T>,
{
    /// Create a new deploy builder from a `web3` provider, artifact data and
    /// deployment (constructor) parameters.
    pub fn new<P>(web3: Web3<T>, factory: F, params: P) -> Result<Self, DeployError>
    where
        P: Tokenize,
    {
        // NOTE(nlordell): unfortunately here we have to re-implement some
        //   `rust-web3` code so that we can add things like signing support;
        //   luckily most of complicated bits can be reused from the tx code

        if factory.bytecode().is_empty() {
            return Err(DeployError::EmptyBytecode);
        }

        let code = factory.bytecode().to_bytes()?;
        let params = params.into_tokens();
        let data = match (factory.abi().constructor(), params.is_empty()) {
            (None, false) => return Err(AbiErrorKind::InvalidData.into()),
            (None, true) => code,
            (Some(ctor), _) => Bytes(ctor.encode_input(code.0, &params)?),
        };

        Ok(DeployBuilder {
            web3: web3.clone(),
            factory,
            tx: TransactionBuilder::new(web3).data(data).confirmations(0),
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
    pub fn gas_price(mut self, value: U256) -> Self {
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
    pub fn deploy(self) -> DeployFuture<T, F> {
        DeployFuture::from_builder(self)
    }
}

/// Future for deploying a contract instance.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct DeployFuture<T, F>
where
    T: Transport,
    F: Factory<T>,
{
    ///
    /// The deployment args
    args: Option<(Web3Unpin<T>, F)>,
    /// The future resolved when the deploy transaction is complete.
    send: SendFuture<T>,
}

impl<T, F> DeployFuture<T, F>
where
    T: Transport,
    F: Factory<T>,
{
    /// Create an instance from a `DeployBuilder`.
    pub fn from_builder(builder: DeployBuilder<T, F>) -> Self {
        DeployFuture {
            args: Some((builder.web3.into(), builder.factory)),
            send: builder.tx.send(),
        }
    }
}

impl<T, F> Future for DeployFuture<T, F>
where
    T: Transport,
    F: Factory<T>,
{
    type Output = Result<F::Instance, DeployError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let unpinned = self.get_mut();

        let tx = match ready!(Pin::new(&mut unpinned.send).poll(cx)) {
            Ok(TransactionResult::Receipt(tx)) => tx,
            Ok(TransactionResult::Hash(tx)) => return Poll::Ready(Err(DeployError::Pending(tx))),
            Err(err) => return Poll::Ready(Err(err.into())),
        };

        let address = match tx.contract_address {
            Some(address) => address,
            None => {
                return Poll::Ready(Err(DeployError::Tx(ExecutionError::Failure(
                    tx.transaction_hash,
                ))));
            }
        };

        let (web3, factory) = unpinned.args.take().expect("called more than once");

        Poll::Ready(Ok(factory.at_address(web3.into(), address)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{Networks, Linker};
    use crate::test::prelude::*;
    use ethcontract_common::truffle::Network;
    use ethcontract_common::{Artifact, Bytecode};

    #[test]
    fn deployed() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let network_id = "42";
        let address = addr!("0x0102030405060708091011121314151617181920");
        let artifact = {
            let mut artifact = Artifact::empty();
            artifact
                .networks
                .insert(network_id.to_string(), Network { address });
            artifact
        };

        transport.add_response(json!(network_id)); // estimate gas response
        let networks = Networks::new(artifact);
        let instance = DeployedFuture::new(web3, networks)
            .wait()
            .expect("successful deployment");

        transport.assert_request("net_version", &[]);
        transport.assert_no_more_requests();

        assert_eq!(instance.address(), address);
    }

    #[test]
    fn deploy_tx_options() {
        let transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let from = addr!("0x9876543210987654321098765432109876543210");
        let bytecode = Bytecode::from_hex_str("0x42").unwrap();
        let artifact = Artifact {
            bytecode: bytecode.clone(),
            ..Artifact::empty()
        };
        let linker = Linker::new(artifact);
        let tx = DeployBuilder::new(web3, linker, ())
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

        let artifact = Artifact::empty();
        let linker = Linker::new(artifact);
        let error = DeployBuilder::new(web3, linker, ())
            .err()
            .unwrap();

        assert_eq!(error.to_string(), DeployError::EmptyBytecode.to_string());
        transport.assert_no_more_requests();
    }
}
