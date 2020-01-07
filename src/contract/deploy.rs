//! Implementation for creating instances for deployed contracts and deploying
//! new contracts.

use crate::contract::Instance;
use crate::errors::DeployError;
use crate::future::{CompatCallFuture, Web3Unpin};
use crate::transaction::{Account, SendAndConfirmFuture, TransactionBuilder};
use crate::truffle::abi::ErrorKind as AbiErrorKind;
use crate::truffle::{Abi, Artifact};
use futures::compat::Future01CompatExt;
use futures::ready;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use web3::api::Web3;
use web3::contract::tokens::Tokenize;
use web3::types::{Address, Bytes, U256};
use web3::Transport;

/// A trait for deployable contract types. This allows generated types to be
/// deployable without having to create new future types.
pub trait Deploy<T: Transport> {
    /// Construct a contract type deployed at an address.
    fn deployed_at(web3: Web3<T>, abi: Abi, at: Address) -> Self;
}

impl<T: Transport> Deploy<T> for Instance<T> {
    #[inline(always)]
    fn deployed_at(web3: Web3<T>, abi: Abi, at: Address) -> Self {
        Instance::at(web3, abi, at)
    }
}

/// Future for creating a deployed contract instance.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct DeployedFuture<T, D>
where
    T: Transport,
    D: Deploy<T>,
{
    /// Deployed arguments: `web3` provider and artifact.
    args: Option<(Web3Unpin<T>, Artifact)>,
    /// Underlying future for retrieving the network ID.
    network_id: CompatCallFuture<T, String>,
    _deploy: PhantomData<Box<D>>,
}

impl<T, D> DeployedFuture<T, D>
where
    T: Transport,
    D: Deploy<T>,
{
    /// Construct a new future that resolves when a deployed contract is located
    /// from a `web3` provider and artifact data.
    pub fn from_args(web3: Web3<T>, artifact: Artifact) -> DeployedFuture<T, D> {
        let net = web3.net();
        DeployedFuture {
            args: Some((web3.into(), artifact)),
            network_id: net.version().compat(),
            _deploy: Default::default(),
        }
    }

    /// Take value of our passed in `web3` provider.
    fn args(self: Pin<&mut Self>) -> (Web3<T>, Artifact) {
        let (web3, artifact) = self
            .get_mut()
            .args
            .take()
            .expect("should be called only once");
        (web3.into(), artifact)
    }

    /// Get a pinned reference to the inner `CallFuture` for retrieving the
    /// current network ID.
    fn network_id(self: Pin<&mut Self>) -> Pin<&mut CompatCallFuture<T, String>> {
        Pin::new(&mut self.get_mut().network_id)
    }
}

impl<T, D> Future for DeployedFuture<T, D>
where
    T: Transport,
    D: Deploy<T>,
{
    type Output = Result<D, DeployError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.as_mut().network_id().poll(cx).map(|network_id| {
            let network_id = network_id?;
            let (web3, artifact) = self.args();

            let address = match artifact.networks.get(&network_id) {
                Some(network) => network.address,
                None => return Err(DeployError::NotFound(network_id)),
            };

            Ok(D::deployed_at(web3, artifact.abi, address))
        })
    }
}

/// Builder for specifying options for deploying a linked contract.
#[derive(Debug, Clone)]
#[must_use = "deploy builers do nothing unless you `.deploy()` them"]
pub struct DeployBuilder<T, D>
where
    T: Transport,
    D: Deploy<T>,
{
    /// The underlying `web3` provider.
    web3: Web3<T>,
    /// The ABI for the contract that is to be deployed.
    abi: Abi,
    /// The underlying transaction used t
    tx: TransactionBuilder<T>,
    /// The poll interval for confirming the contract deployed.
    pub poll_interval: Option<Duration>,
    /// The number of confirmations to wait for.
    pub confirmations: Option<usize>,
    _deploy: PhantomData<Box<D>>,
}

impl<T, D> DeployBuilder<T, D>
where
    T: Transport,
    D: Deploy<T>,
{
    /// Create a new deploy builder from a `web3` provider, artifact data and
    /// deployment (constructor) parameters.
    pub fn new<P>(
        web3: Web3<T>,
        artifact: Artifact,
        params: P,
    ) -> Result<DeployBuilder<T, D>, DeployError>
    where
        P: Tokenize,
    {
        // NOTE(nlordell): unfortunately here we have to re-implement some
        //   `rust-web3` code so that we can add things like signing support;
        //   luckily most of complicated bits can be reused from the tx code

        if artifact.bytecode.is_empty() {
            return Err(DeployError::EmptyBytecode);
        }

        let code = artifact.bytecode.into_bytes()?;
        let params = params.into_tokens();
        let data = match (artifact.abi.constructor(), params.is_empty()) {
            (None, false) => return Err(AbiErrorKind::InvalidData.into()),
            (None, true) => code,
            (Some(ctor), _) => Bytes(ctor.encode_input(code.0, &params)?),
        };

        Ok(DeployBuilder {
            web3: web3.clone(),
            abi: artifact.abi,
            tx: TransactionBuilder::new(web3).data(data),
            poll_interval: None,
            confirmations: None,
            _deploy: Default::default(),
        })
    }

    /// Specify the signing method to use for the transaction, if not specified
    /// the the transaction will be locally signed with the default user.
    pub fn from(mut self, value: Account) -> DeployBuilder<T, D> {
        self.tx = self.tx.from(value);
        self
    }

    /// Secify amount of gas to use, if not specified then a gas estimate will
    /// be used.
    pub fn gas(mut self, value: U256) -> DeployBuilder<T, D> {
        self.tx = self.tx.gas(value);
        self
    }

    /// Specify the gas price to use, if not specified then the estimated gas
    /// price will be used.
    pub fn gas_price(mut self, value: U256) -> DeployBuilder<T, D> {
        self.tx = self.tx.gas_price(value);
        self
    }

    /// Specify what how much ETH to transfer with the transaction, if not
    /// specified then no ETH will be sent.
    pub fn value(mut self, value: U256) -> DeployBuilder<T, D> {
        self.tx = self.tx.value(value);
        self
    }

    /// Specify the nonce for the transation, if not specified will use the
    /// current transaction count for the signing account.
    pub fn nonce(mut self, value: U256) -> DeployBuilder<T, D> {
        self.tx = self.tx.nonce(value);
        self
    }

    /// Specify the poll interval to use for confirming the deployment, if not
    /// specified will use a period of 7 seconds.
    pub fn poll_interval(mut self, value: Duration) -> DeployBuilder<T, D> {
        self.poll_interval = Some(value);
        self
    }

    /// Specify the number of confirmations to wait for when confirming the
    /// transaction, if not specified will wait for 1 confirmation.
    pub fn confirmations(mut self, value: usize) -> DeployBuilder<T, D> {
        self.confirmations = Some(value);
        self
    }

    /// Extract inner `TransactionBuilder` from this `DeployBuilder`. This
    /// exposes `TransactionBuilder` only APIs.
    pub fn into_inner(self) -> TransactionBuilder<T> {
        self.tx
    }

    /// Sign (if required) and execute the transaction. Returns the transaction
    /// hash that can be used to retrieve transaction information.
    pub fn deploy(self) -> DeployFuture<T, D> {
        DeployFuture::from_builder(self)
    }
}

/// Future for deploying a contract instance.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct DeployFuture<T, D>
where
    T: Transport,
    D: Deploy<T>,
{
    /// The deployment args
    args: Option<(Web3Unpin<T>, Abi)>,
    /// The future resolved when the deploy transaction is complete.
    tx: Result<SendAndConfirmFuture<T>, Option<DeployError>>,
    _deploy: PhantomData<Box<D>>,
}

impl<T, D> DeployFuture<T, D>
where
    T: Transport,
    D: Deploy<T>,
{
    /// Create an instance from a `DeployBuilder`.
    pub fn from_builder(builder: DeployBuilder<T, D>) -> DeployFuture<T, D> {
        // NOTE(nlordell): arbitrary default values taken from `rust-web3`
        let poll_interval = builder.poll_interval.unwrap_or(Duration::from_secs(7));
        let confirmations = builder.confirmations.unwrap_or(1);

        DeployFuture {
            args: Some((builder.web3.into(), builder.abi)),
            tx: Ok(builder.tx.send_and_confirm(poll_interval, confirmations)),
            _deploy: Default::default(),
        }
    }
}

impl<T, D> Future for DeployFuture<T, D>
where
    T: Transport,
    D: Deploy<T>,
{
    type Output = Result<D, DeployError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let unpinned = self.get_mut();
        match unpinned.tx {
            Ok(ref mut tx) => {
                let tx = ready!(Pin::new(tx).poll(cx).map_err(DeployError::from));
                let tx = match tx {
                    Ok(tx) => tx,
                    Err(err) => return Poll::Ready(Err(err)),
                };
                let address = match tx.contract_address {
                    Some(address) => address,
                    None => {
                        return Poll::Ready(Err(DeployError::Failure(tx.transaction_hash)));
                    }
                };

                let (web3, abi) = unpinned.args.take().expect("called once");

                Poll::Ready(Ok(D::deployed_at(web3.into(), abi, address)))
            }
            Err(ref mut err) => Poll::Ready(Err(err.take().expect("called once"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::Instance;
    use crate::test::prelude::*;
    use crate::truffle::{Artifact, Bytecode, Network};

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
        let instance: Instance<_> = DeployedFuture::from_args(web3, artifact)
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
        let tx = DeployBuilder::<_, Instance<_>>::new(web3, artifact, ())
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
        assert_eq!(tx.data, Some(bytecode.into_bytes().unwrap()));
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
        let error = DeployBuilder::<_, Instance<_>>::new(web3, artifact, ())
            .err()
            .unwrap();

        assert_eq!(error.to_string(), DeployError::EmptyBytecode.to_string());
        transport.assert_no_more_requests();
    }
}
