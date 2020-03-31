//! Implementation for creating instances for deployed contracts.

use crate::errors::DeployError;
use crate::future::CompatCallFuture;
use futures::compat::Future01CompatExt;
use pin_project::pin_project;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use web3::api::Web3;
use web3::Transport;

/// a factory trait for deployable contract instances. this traits provides
/// functionality for creating instances of a contract type for a given network
/// ID.
///
/// this allows generated contracts to be deployable without having to create
/// new builder and future types.
pub trait FromNetwork<T: Transport>: Sized {
    /// Context passed to the `Deployments`.
    type Context;

    /// Create a contract instance for the specified network. This method should
    /// return `None` when no deployment can be found for the specified network
    /// ID.
    fn from_network(web3: Web3<T>, network_id: &str, cx: Self::Context) -> Option<Self>;
}

/// Future for creating a deployed contract instance.
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[pin_project]
pub struct DeployedFuture<T, I>
where
    T: Transport,
    I: FromNetwork<T>,
{
    /// The deployment arguments.
    args: Option<(Web3<T>, I::Context)>,
    /// The factory used to locate the contract address from a netowkr ID.
    /// Underlying future for retrieving the network ID.
    #[pin]
    network_id: CompatCallFuture<T, String>,
    _instance: PhantomData<Box<I>>,
}

impl<T, I> DeployedFuture<T, I>
where
    T: Transport,
    I: FromNetwork<T>,
{
    /// Construct a new future that resolves when a deployed contract is located
    /// from a `web3` provider and artifact data.
    pub fn new(web3: Web3<T>, context: I::Context) -> Self {
        let net = web3.net();
        DeployedFuture {
            args: Some((web3, context)),
            network_id: net.version().compat(),
            _instance: PhantomData,
        }
    }
}

impl<T, I> Future for DeployedFuture<T, I>
where
    T: Transport,
    I: FromNetwork<T>,
{
    type Output = Result<I, DeployError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.as_mut()
            .project()
            .network_id
            .poll(cx)
            .map(|network_id| {
                let network_id = network_id?;
                let (web3, context) = self.args.take().expect("called more than once");
                I::from_network(web3, &network_id, context).ok_or(DeployError::NotFound(network_id))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{Deployments, Instance};
    use crate::test::prelude::*;
    use ethcontract_common::truffle::Network;
    use ethcontract_common::Artifact;
    use web3::types::H256;

    type InstanceDeployedFuture<T> = DeployedFuture<T, Instance<T>>;

    #[test]
    fn deployed() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let network_id = "42";
        let address = addr!("0x0102030405060708091011121314151617181920");
        let transaction_hash = Some(H256::repeat_byte(0x42));
        let artifact = {
            let mut artifact = Artifact::empty();
            artifact.networks.insert(
                network_id.to_string(),
                Network {
                    address,
                    transaction_hash,
                },
            );
            artifact
        };

        transport.add_response(json!(network_id)); // get network ID response
        let networks = Deployments::new(artifact);
        let instance = InstanceDeployedFuture::new(web3, networks)
            .immediate()
            .expect("successful deployment");

        transport.assert_request("net_version", &[]);
        transport.assert_no_more_requests();

        assert_eq!(instance.address(), address);
        assert_eq!(instance.transaction_hash(), transaction_hash);
    }

    #[test]
    fn deployed_not_found() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let network_id = "42";

        transport.add_response(json!(network_id)); // get network ID response
        let networks = Deployments::new(Artifact::empty());
        let err = InstanceDeployedFuture::new(web3, networks)
            .immediate()
            .expect_err("unexpected success getting deployed contract");

        transport.assert_request("net_version", &[]);
        transport.assert_no_more_requests();

        assert!(
            match &err {
                DeployError::NotFound(id) => id == network_id,
                _ => false,
            },
            "expected network {} not found error but got '{:?}'",
            network_id,
            err
        );
    }
}
