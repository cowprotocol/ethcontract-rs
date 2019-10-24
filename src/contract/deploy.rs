//! Implementation for creating instances for deployed contracts and deploying
//! new contracts.

use crate::contract::Instance;
use crate::errors::DeployError;
use crate::future::{CompatCallFuture, Web3Unpin};
use crate::transaction::{Account, TransactionBuilder};
use crate::truffle::{Abi, Artifact, Bytecode};
use ethabi::{ErrorKind as AbiErrorKind, Token};
use futures::compat::Future01CompatExt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use web3::api::Web3;
use web3::contract::tokens::Tokenize;
use web3::types::{Address, Bytes, U256};
use web3::Transport;

/// Future for creating a deployed contract instance.
pub struct DeployedFuture<T: Transport> {
    /// Deployed arguments: `web3` provider and artifact.
    args: Option<(Web3Unpin<T>, Artifact)>,
    /// Underlying future for retrieving the network ID.
    network_id: CompatCallFuture<T, String>,
}

impl<T: Transport> DeployedFuture<T> {
    pub(crate) fn from_args(web3: Web3<T>, artifact: Artifact) -> DeployedFuture<T> {
        let net = web3.net();
        DeployedFuture {
            args: Some((web3.into(), artifact)),
            network_id: net.version().compat(),
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

impl<T: Transport> Future for DeployedFuture<T> {
    type Output = Result<Instance<T>, DeployError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.as_mut().network_id().poll(cx).map(|network_id| {
            let network_id = network_id?;
            let (web3, artifact) = self.args();

            let address = match artifact.networks.get(&network_id) {
                Some(network) => network.address,
                None => return Err(DeployError::NotFound(network_id)),
            };

            Ok(Instance {
                web3,
                abi: artifact.abi,
                address,
            })
        })
    }
}

/// Builder for specifying options for deploying a contract.
#[derive(Debug, Clone)]
pub struct DeployBuilder<T: Transport> {
    /// The deployment code for the contract.
    code: Bytecode,
    /// The ABI for the contract that is to be deployed.
    abi: Abi,
    /// The tokenized parameters.
    params: Vec<Token>,
    /// The underlying transaction used t
    tx: TransactionBuilder<T>,
}

impl<T: Transport> DeployBuilder<T> {
    pub(crate) fn new<P>(web3: Web3<T>, artifact: Artifact, params: P) -> DeployBuilder<T>
    where
        P: Tokenize,
    {
        // NOTE(nlordell): unfortunately here we have to re-implement some
        //   `rust-web3` code so that we can add things like signing support;
        //   luckily most of complicated bits can be reused from the tx code

        DeployBuilder {
            code: artifact.bytecode,
            abi: artifact.abi,
            params: params.into_tokens(),
            tx: TransactionBuilder::new(web3),
        }
    }

    /// Specify a linked library used for this contract. Note that we
    /// incrementally link so that we can verify each time a library is linked
    /// whether it was successful or not.
    ///
    /// # Panics
    ///
    /// Panics if an invalid library name is used (for example if it is more
    /// than 38 characters long).
    pub fn link<S>(mut self, name: S, address: Address) -> Result<DeployBuilder<T>, DeployError>
    where
        S: AsRef<str>,
    {
        self.code.link(name, address)?;
        Ok(self)
    }

    /// Commit deploy options into inner transaction and return remaining parts.
    fn commit(self) -> Result<(Abi, TransactionBuilder<T>), DeployError> {
        let code = self.code.into_bytes()?;
        let data = match (self.abi.constructor(), self.params.is_empty()) {
            (None, false) => return Err(AbiErrorKind::InvalidData.into()),
            (None, true) => code,
            (Some(ctor), _) => Bytes(ctor.encode_input(code.0, &self.params)?),
        };

        Ok((self.abi, self.tx.data(data)))
    }

    /// Specify the signing method to use for the transaction, if not specified
    /// the the transaction will be locally signed with the default user.
    pub fn from(mut self, value: Account) -> DeployBuilder<T> {
        self.tx = self.tx.from(value);
        self
    }

    /// Secify amount of gas to use, if not specified then a gas estimate will
    /// be used.
    pub fn gas(mut self, value: U256) -> DeployBuilder<T> {
        self.tx = self.tx.gas(value);
        self
    }

    /// Specify the gas price to use, if not specified then the estimated gas
    /// price will be used.
    pub fn gas_price(mut self, value: U256) -> DeployBuilder<T> {
        self.tx = self.tx.gas(value);
        self
    }

    /// Specify what how much ETH to transfer with the transaction, if not
    /// specified then no ETH will be sent.
    pub fn value(mut self, value: U256) -> DeployBuilder<T> {
        self.tx = self.tx.gas(value);
        self
    }

    /// Specify the nonce for the transation, if not specified will use the
    /// current transaction count for the signing account.
    pub fn nonce(mut self, value: U256) -> DeployBuilder<T> {
        self.tx = self.tx.gas(value);
        self
    }

    /// Extract inner `TransactionBuilder` from this `DeployBuilder`. This exposes
    /// `TransactionBuilder` only APIs such as `estimate_gas` and `build`.
    pub fn into_inner(self) -> Result<TransactionBuilder<T>, DeployError> {
        let (_, tx) = self.commit()?;
        Ok(tx)
    }

    /// Sign (if required) and execute the transaction. Returns the transaction
    /// hash that can be used to retrieve transaction information.
    pub fn deploy(self) -> DeployFuture<T> {
        DeployFuture::from_builder(self)
    }
}

/// Builder for specifying options for deploying a linked contract.
#[derive(Debug, Clone)]
pub struct LinkedDeployBuilder<T: Transport> {
    /// The contract ABI.
    abi: Abi,
    /// The underlying transaction used t
    tx: TransactionBuilder<T>,
}

impl<T: Transport> LinkedDeployBuilder<T> {
    pub(crate) fn new<'a, P, I>(
        web3: Web3<T>,
        artifact: Artifact,
        params: P,
        libraries: I,
    ) -> Result<LinkedDeployBuilder<T>, DeployError>
    where
        P: Tokenize,
        I: Iterator<Item = (&'a str, Address)>,
    {
        let mut builder = DeployBuilder::new(web3, artifact, params);
        for (name, address) in libraries {
            builder = builder.link(name, address)?;
        }

        let (abi, tx) = builder.commit()?;

        Ok(LinkedDeployBuilder { abi, tx })
    }

    /// Specify the signing method to use for the transaction, if not specified
    /// the the transaction will be locally signed with the default user.
    pub fn from(mut self, value: Account) -> LinkedDeployBuilder<T> {
        self.tx = self.tx.from(value);
        self
    }

    /// Secify amount of gas to use, if not specified then a gas estimate will
    /// be used.
    pub fn gas(mut self, value: U256) -> LinkedDeployBuilder<T> {
        self.tx = self.tx.gas(value);
        self
    }

    /// Specify the gas price to use, if not specified then the estimated gas
    /// price will be used.
    pub fn gas_price(mut self, value: U256) -> LinkedDeployBuilder<T> {
        self.tx = self.tx.gas(value);
        self
    }

    /// Specify what how much ETH to transfer with the transaction, if not
    /// specified then no ETH will be sent.
    pub fn value(mut self, value: U256) -> LinkedDeployBuilder<T> {
        self.tx = self.tx.gas(value);
        self
    }

    /// Specify the nonce for the transation, if not specified will use the
    /// current transaction count for the signing account.
    pub fn nonce(mut self, value: U256) -> LinkedDeployBuilder<T> {
        self.tx = self.tx.gas(value);
        self
    }

    /// Extract inner `TransactionBuilder` from this `LinkedDeployBuilder`. This
    /// exposes `TransactionBuilder` only APIs.
    pub fn into_inner(self) -> TransactionBuilder<T> {
        self.tx
    }

    /// Sign (if required) and execute the transaction. Returns the transaction
    /// hash that can be used to retrieve transaction information.
    pub fn deploy(self) -> DeployFuture<T> {
        DeployFuture::from_linked_builder(self)
    }
}

/// Future for deploying a contract instance.
pub struct DeployFuture<T: Transport> {
    /// Deployed arguments: `web3` provider and artifact.
    args: Option<(Web3Unpin<T>, Artifact)>,
    /// Underlying future for retrieving the network ID.
    network_id: CompatCallFuture<T, String>,
}

impl<T: Transport> DeployFuture<T> {
    /// Create an instance from a `DeployBuilder`.
    pub fn from_builder(builder: DeployBuilder<T>) -> DeployFuture<T> {
        let tx = builder.into_inner();

        let _ = tx;
        unimplemented!()
    }

    /// Create an instance from a `LinkedDeployBuilder`.
    pub fn from_linked_builder(builder: LinkedDeployBuilder<T>) -> DeployFuture<T> {
        let tx = builder.into_inner();

        let _ = tx;
        unimplemented!()
    }
}
