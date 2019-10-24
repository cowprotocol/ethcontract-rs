//! Abtraction for interacting with ethereum smart contracts. Provides methods
//! for sending transactions to contracts as well as querying current contract
//! state.

mod call;
mod deploy;
mod send;

use crate::truffle::{Abi, Artifact};
use crate::errors::DeployError;
use ethabi::{Function, Result as AbiResult};
use web3::api::Web3;
use web3::contract::tokens::{Detokenize, Tokenize};
use web3::types::{Address, Bytes};
use web3::Transport;

pub use self::call::{CallBuilder, ExecuteCallFuture};
pub use self::deploy::{DeployBuilder, DeployFuture, DeployedFuture, LinkedDeployBuilder};
pub use self::send::SendBuilder;

/// Represents a contract instance at an address. Provides methods for
/// contract interaction.
pub struct Instance<T: Transport> {
    web3: Web3<T>,
    abi: Abi,
    address: Address,
}

impl<T: Transport> Instance<T> {
    /// Creates a new contract instance with the specified `web3` provider with
    /// the given `Abi` at the given `Address`.
    ///
    /// Note that this does not verify that a contract with a matchin `Abi` is
    /// actually deployed at the given address.
    pub fn at(web3: Web3<T>, abi: Abi, address: Address) -> Instance<T> {
        Instance { web3, abi, address }
    }

    /// Locates a deployed contract based on the current network ID reported by
    /// the `web3` provider from the given `Artifact`'s ABI and networks.
    ///
    /// Note that this does not verify that a contract with a matchin `Abi` is
    /// actually deployed at the given address.
    pub fn deployed(web3: Web3<T>, artifact: Artifact) -> DeployedFuture<T> {
        DeployedFuture::from_args(web3, artifact)
    }

    /// Deploys a contract with the specified `web3` provider with the given
    /// `Artifact` byte code.
    pub fn deploy<P>(web3: Web3<T>, artifact: Artifact, params: P) -> DeployBuilder<T>
    where
        P: Tokenize,
    {
        DeployBuilder::new(web3, artifact, params)
    }

    /// Deploys a contract with the specified `web3` provider with the given
    /// `Artifact` byte code and linking libraries.
    pub fn deploy_linked<'a, P, I>(
        web3: Web3<T>,
        artifact: Artifact,
        params: P,
        libraries: I,
    ) -> Result<LinkedDeployBuilder<T>, DeployError>
    where
        P: Tokenize,
        I: Iterator<Item = (&'a str, Address)>,
    {
        LinkedDeployBuilder::new(web3, artifact, params, libraries)
    }

    /// Create a clone of the handle to our current `web3` provider.
    fn web3(&self) -> Web3<T> {
        self.web3.clone()
    }

    /// Returns the contract address being used by this instance.
    pub fn address(&self) -> Address {
        self.address
    }

    /// Returns a call builder to setup a query to a smart contract that just
    /// gets evaluated on a node but does not actually commit anything to the
    /// block chain.
    pub fn call<S, P, R>(&self, name: S, params: P) -> AbiResult<CallBuilder<T, R>>
    where
        S: AsRef<str>,
        P: Tokenize,
        R: Detokenize,
    {
        let (function, data) = self.encode_abi(name, params)?;

        // take ownership here as it greatly simplifies dealing with futures
        // lifetime as it would require the contract Instance to live until
        // the end of the future
        let function = function.clone();

        Ok(CallBuilder::new(self.web3(), function, self.address, data))
    }

    /// Returns a transaction builder to setup a transaction
    pub fn send<S, P>(&self, name: S, params: P) -> AbiResult<SendBuilder<T>>
    where
        S: AsRef<str>,
        P: Tokenize,
    {
        let (_, data) = self.encode_abi(name, params)?;
        Ok(SendBuilder::new(self.web3(), self.address, data))
    }

    /// Utility function to locate a function by name and encode the function
    /// signature and parameters into data bytes to be sent to a contract.
    #[inline(always)]
    fn encode_abi<S, P>(&self, name: S, params: P) -> AbiResult<(&Function, Bytes)>
    where
        S: AsRef<str>,
        P: Tokenize,
    {
        let function = self.abi.function(name.as_ref())?;
        let data = function.encode_input(&params.into_tokens())?;

        Ok((function, data.into()))
    }
}
