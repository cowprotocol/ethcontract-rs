//! Module for reading and examining data produced by truffle.

use crate::abiext::FunctionExt;
use crate::hash::H32;
use crate::Abi;
use crate::{bytecode::Bytecode, DeploymentInformation};
use ethabi::ethereum_types::H256;
use serde::Deserializer;
use serde::Serializer;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;
use std::sync::Arc;
use web3::types::Address;

/// Represents a contract data.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default = "Contract::empty")]
pub struct Contract {
    /// The contract name. Unnamed contracts have an empty string as their name.
    #[serde(rename = "contractName")]
    pub name: String,
    /// The contract interface.
    #[serde(rename = "abi")]
    pub interface: Arc<Interface>,
    /// The contract deployment bytecode.
    pub bytecode: Bytecode,
    /// The contract's expected deployed bytecode.
    #[serde(rename = "deployedBytecode")]
    pub deployed_bytecode: Bytecode,
    /// The configured networks by network ID for the contract.
    pub networks: HashMap<String, Network>,
    /// The developer documentation.
    pub devdoc: Documentation,
    /// The user documentation.
    pub userdoc: Documentation,
}

/// Struct representing publicly accessible interface of a smart contract.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Interface {
    /// The contract ABI
    pub abi: Abi,
    /// A mapping from method signature to a name-index pair for accessing
    /// functions in the contract ABI. This is used to avoid allocation when
    /// searching for matching functions by signature.
    pub methods: HashMap<H32, (String, usize)>,
    /// A mapping from event signature to a name-index pair for resolving
    /// events in the contract ABI.
    pub events: HashMap<H256, (String, usize)>,
}

impl<'de> Deserialize<'de> for Interface {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let abi = Abi::deserialize(deserializer)?;
        Ok(abi.into())
    }
}

impl Serialize for Interface {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.abi.serialize(serializer)
    }
}

impl From<Abi> for Interface {
    fn from(abi: Abi) -> Self {
        Self {
            methods: create_mapping(&abi.functions, |function| function.selector()),
            events: create_mapping(&abi.events, |event| event.signature()),
            abi,
        }
    }
}

/// Utility function for creating a mapping between a unique signature and a
/// name-index pair for accessing contract ABI items.
fn create_mapping<T, S, F>(
    elements: &BTreeMap<String, Vec<T>>,
    signature: F,
) -> HashMap<S, (String, usize)>
where
    S: Hash + Eq + Ord,
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

impl Contract {
    /// Creates an empty contract instance.
    pub fn empty() -> Self {
        Contract::with_name(String::default())
    }

    /// Creates an empty contract instance with the given name.
    pub fn with_name(name: impl Into<String>) -> Self {
        Contract {
            name: name.into(),
            interface: Default::default(),
            bytecode: Default::default(),
            deployed_bytecode: Default::default(),
            networks: HashMap::new(),
            devdoc: Default::default(),
            userdoc: Default::default(),
        }
    }
}

/// A contract's network configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Network {
    /// The address at which the contract is deployed on this network.
    pub address: Address,
    /// The hash of the transaction that deployed the contract on this network.
    #[serde(rename = "transactionHash")]
    pub deployment_information: Option<DeploymentInformation>,
}

/// A contract's documentation.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Documentation {
    /// Contract documentation
    pub details: Option<String>,
    /// Contract method documentation.
    pub methods: HashMap<String, DocEntry>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
/// A documentation entry.
pub struct DocEntry {
    /// The documentation details for this entry.
    pub details: Option<String>,
}
