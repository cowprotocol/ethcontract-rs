//! Abtraction for interacting with ethereum smart contracts. Provides methods
//! for sending transactions to contracts as well as querying current contract
//! state.

mod deploy;
mod event;
mod method;

use crate::{
    errors::{DeployError, LinkError},
    tokens::Tokenize,
};
use ethcontract_common::abi::{Error as AbiError, Result as AbiResult};
use ethcontract_common::abiext::FunctionExt;
use ethcontract_common::hash::H32;
use ethcontract_common::{Abi, Contract, Bytecode, DeploymentInformation};
use std::collections::HashMap;
use std::hash::Hash;
use web3::api::Web3;
use web3::types::{Address, Bytes, H256};
use web3::Transport;

pub use self::deploy::{Deploy, DeployBuilder};
pub use self::event::{
    AllEventsBuilder, Event, EventBuilder, EventMetadata, EventStatus, ParseLog, RawLog,
    StreamEvent, Topic,
};
pub use self::method::{MethodBuilder, MethodDefaults, ViewMethodBuilder};

/// Represents a contract instance at an address. Provides methods for
/// contract interaction.
#[derive(Debug, Clone)]
pub struct Instance<T: Transport> {
    web3: Web3<T>,
    abi: Abi,
    address: Address,
    deployment_information: Option<DeploymentInformation>,
    /// Default method parameters to use when sending method transactions or
    /// querying method calls.
    pub defaults: MethodDefaults,
    /// A mapping from method signature to a name-index pair for accessing
    /// functions in the contract ABI. This is used to avoid allocation when
    /// searching for matching functions by signature.
    methods: HashMap<H32, (String, usize)>,
    /// A mapping from event signature to a name-index pair for resolving
    /// events in the contract ABI.
    events: HashMap<H256, (String, usize)>,
}

impl<T: Transport> Instance<T> {
    /// Creates a new contract instance with the specified `web3` provider with
    /// the given `Abi` at the given `Address`.
    ///
    /// Note that this does not verify that a contract with a matching `Abi` is
    /// actually deployed at the given address.
    pub fn at(web3: Web3<T>, abi: Abi, address: Address) -> Self {
        Instance::with_deployment_info(web3, abi, address, None)
    }

    /// Creates a new contract instance with the specified `web3` provider with
    /// the given `Abi` at the given `Address` and an optional transaction hash.
    /// This hash is used to retrieve contract related information such as the
    /// creation block (which is useful for fetching all historic events).
    ///
    /// Note that this does not verify that a contract with a matching `Abi` is
    /// actually deployed at the given address nor that the transaction hash,
    /// when provided, is actually for this contract deployment.
    pub fn with_deployment_info(
        web3: Web3<T>,
        abi: Abi,
        address: Address,
        deployment_information: Option<DeploymentInformation>,
    ) -> Self {
        let methods = create_mapping(&abi.functions, |function| function.selector());
        let events = create_mapping(&abi.events, |event| event.signature());

        Instance {
            web3,
            abi,
            address,
            deployment_information,
            defaults: MethodDefaults::default(),
            methods,
            events,
        }
    }

    /// Locates a deployed contract based on the current network ID reported by
    /// the `web3` provider from the given `Contract`'s ABI and networks.
    ///
    /// Note that this does not verify that a contract with a matching `Abi` is
    /// actually deployed at the given address.
    pub async fn deployed(web3: Web3<T>, contract: Contract) -> Result<Self, DeployError> {
        let network_id = web3.net().version().await?;
        let network = contract
            .networks
            .get(&network_id)
            .ok_or(DeployError::NotFound(network_id))?;

        Ok(Instance::with_deployment_info(
            web3,
            contract.abi,
            network.address,
            network.deployment_information,
        ))
    }

    /// Creates a contract builder with the specified `web3` provider and the
    /// given `Contract` byte code. This allows the contract deployment
    /// transaction to be configured before deploying the contract.
    pub fn builder<P>(
        web3: Web3<T>,
        contract: Contract,
        params: P,
    ) -> Result<DeployBuilder<T, Self>, DeployError>
    where
        P: Tokenize,
    {
        Linker::new(contract).deploy(web3, params)
    }

    /// Deploys a contract with the specified `web3` provider with the given
    /// `Contract` byte code and linking libraries.
    pub fn link_and_deploy<'a, P, I>(
        web3: Web3<T>,
        contract: Contract,
        params: P,
        libraries: I,
    ) -> Result<DeployBuilder<T, Self>, DeployError>
    where
        P: Tokenize,
        I: Iterator<Item = (&'a str, Address)>,
    {
        let mut linker = Linker::new(contract);
        for (name, address) in libraries {
            linker = linker.library(name, address)?;
        }

        linker.deploy(web3, params)
    }

    /// Retrieve the underlying web3 provider used by this contract instance.
    pub fn web3(&self) -> Web3<T> {
        self.web3.clone()
    }

    /// Retrieves the contract ABI for this instance.
    pub fn abi(&self) -> &Abi {
        &self.abi
    }

    /// Returns the contract address being used by this instance.
    pub fn address(&self) -> Address {
        self.address
    }

    /// Returns the hash for the transaction that deployed the contract if it is
    /// known, `None` otherwise.
    pub fn deployment_information(&self) -> Option<DeploymentInformation> {
        self.deployment_information
    }

    /// Returns a method builder to setup a call or transaction on a smart
    /// contract method. Note that calls just get evaluated on a node but do not
    /// actually commit anything to the block chain.
    pub fn method<P, R>(&self, signature: H32, params: P) -> AbiResult<MethodBuilder<T, R>>
    where
        P: Tokenize,
        R: Tokenize,
    {
        let signature = signature.as_ref();
        let function = self
            .methods
            .get(signature)
            .map(|(name, index)| &self.abi.functions[name][*index])
            .ok_or_else(|| AbiError::InvalidName(hex::encode(&signature)))?;
        let tokens = match params.into_token() {
            ethcontract_common::abi::Token::Tuple(tokens) => tokens,
            _ => unreachable!("function arguments are always tuples"),
        };
        let data = function.encode_input(&tokens)?;

        // take ownership here as it greatly simplifies dealing with futures
        // lifetime as it would require the contract Instance to live until
        // the end of the future
        let function = function.clone();
        let data = Bytes(data);

        Ok(
            MethodBuilder::new(self.web3(), function, self.address, data)
                .with_defaults(&self.defaults),
        )
    }

    /// Returns a view method builder to setup a call to a smart contract. View
    /// method builders can't actually send transactions and only query contract
    /// state.
    pub fn view_method<P, R>(&self, signature: H32, params: P) -> AbiResult<ViewMethodBuilder<T, R>>
    where
        P: Tokenize,
        R: Tokenize,
    {
        Ok(self.method(signature, params)?.view())
    }

    /// Returns a method builder to setup a call to a smart contract's fallback
    /// function.
    ///
    /// This method will error if the ABI does not contain an entry for a
    /// fallback function.
    pub fn fallback<D>(&self, data: D) -> AbiResult<MethodBuilder<T, ()>>
    where
        D: Into<Vec<u8>>,
    {
        if !self.abi.fallback {
            return Err(AbiError::InvalidName("fallback".into()));
        }

        Ok(MethodBuilder::fallback(
            self.web3(),
            self.address,
            Bytes(data.into()),
        ))
    }

    /// Returns a event builder to setup an event stream for a smart contract
    /// that emits events for the specified Solidity event by name.
    pub fn event<E>(&self, signature: H256) -> AbiResult<EventBuilder<T, E>>
    where
        E: Tokenize,
    {
        let event = self
            .events
            .get(&signature)
            .map(|(name, index)| &self.abi.events[name][*index])
            .ok_or_else(|| AbiError::InvalidName(hex::encode(&signature)))?;

        Ok(EventBuilder::new(
            self.web3(),
            event.clone(),
            self.address(),
        ))
    }

    /// Returns a log stream that emits a log for every new event emitted after
    /// the stream was created for this contract instance.
    pub fn all_events(&self) -> AllEventsBuilder<T, RawLog> {
        AllEventsBuilder::new(self.web3(), self.address(), self.deployment_information())
    }
}

/// Builder for specifying linking options for a contract.
#[derive(Debug, Clone)]
pub struct Linker {
    /// The contract ABI.
    abi: Abi,
    /// The deployment code for the contract.
    bytecode: Bytecode,
}

impl Linker {
    /// Create a new linker for a contract.
    pub fn new(contract: Contract) -> Linker {
        Linker {
            abi: contract.abi,
            bytecode: contract.bytecode,
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
    pub fn library<S>(mut self, name: S, address: Address) -> Result<Linker, LinkError>
    where
        S: AsRef<str>,
    {
        self.bytecode.link(name, address)?;
        Ok(self)
    }

    /// Finish linking and check if there are any outstanding unlinked libraries
    /// and create a deployment builder.
    pub fn deploy<T, P>(
        self,
        web3: Web3<T>,
        params: P,
    ) -> Result<DeployBuilder<T, Instance<T>>, DeployError>
    where
        T: Transport,
        P: Tokenize,
    {
        DeployBuilder::new(web3, self, params)
    }
}

impl<T: Transport> Deploy<T> for Instance<T> {
    type Context = Linker;

    fn abi(cx: &Self::Context) -> &Abi {
        &cx.abi
    }

    fn bytecode(cx: &Self::Context) -> &Bytecode {
        &cx.bytecode
    }

    fn from_deployment(
        web3: Web3<T>,
        address: Address,
        transaction_hash: H256,
        cx: Self::Context,
    ) -> Self {
        Instance::with_deployment_info(
            web3,
            cx.abi,
            address,
            Some(DeploymentInformation::TransactionHash(transaction_hash)),
        )
    }
}

/// Utility function for creating a mapping between a unique signature and a
/// name-index pair for accessing contract ABI items.
fn create_mapping<T, S, F>(
    elements: &HashMap<String, Vec<T>>,
    signature: F,
) -> HashMap<S, (String, usize)>
where
    S: Hash + Eq,
    F: Fn(&T) -> S,
{
    let signature = &signature;
    elements
        .iter()
        .flat_map(|(name, sub_elements)| {
            sub_elements
                .iter()
                .enumerate()
                .map(move |(index, element)| (signature(element), (name.to_owned(), index)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::prelude::*;
    use ethcontract_common::contract::Network;
    use ethcontract_common::Contract;
    use web3::types::H256;

    #[test]
    fn deployed() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let network_id = "42";
        let address = addr!("0x0102030405060708091011121314151617181920");
        let contract = {
            let mut contract = Contract::empty();
            contract.networks.insert(
                network_id.to_string(),
                Network {
                    address,
                    deployment_information: Some(H256::repeat_byte(0x42).into()),
                },
            );
            contract
        };

        transport.add_response(json!(network_id)); // get network ID response
        let instance = Instance::deployed(web3, contract)
            .immediate()
            .expect("successful deployment");

        transport.assert_request("net_version", &[]);
        transport.assert_no_more_requests();

        assert_eq!(instance.address(), address);
        assert_eq!(
            instance.deployment_information(),
            Some(DeploymentInformation::TransactionHash(H256::repeat_byte(
                0x42
            )))
        );
    }

    #[test]
    fn deployed_not_found() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let network_id = "42";

        transport.add_response(json!(network_id)); // get network ID response
        let err = Instance::deployed(web3, Contract::empty())
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
