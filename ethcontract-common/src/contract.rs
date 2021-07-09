//! Module for reading and examining data produced by truffle.

use crate::errors::ArtifactError;
use crate::Abi;
use crate::{bytecode::Bytecode, DeploymentInformation};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use web3::types::Address;

/// Represents a contract data.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default = "Contract::empty")]
pub struct Contract {
    /// The contract name. Unnamed contracts have an empty string as their name.
    #[serde(rename = "contractName")]
    pub name: String,
    /// The contract ABI
    pub abi: Abi,
    /// The contract deployment bytecode.
    pub bytecode: Bytecode,
    /// The configured networks by network ID for the contract.
    pub networks: HashMap<String, Network>,
    /// The developer documentation.
    pub devdoc: Documentation,
    /// The user documentation.
    pub userdoc: Documentation,
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
            abi: Abi {
                constructor: None,
                functions: HashMap::new(),
                events: HashMap::new(),
                fallback: false,
                receive: false,
            },
            bytecode: Default::default(),
            networks: HashMap::new(),
            devdoc: Default::default(),
            userdoc: Default::default(),
        }
    }

    /// Parses a truffle artifact from JSON.
    pub fn from_json<S>(json: S) -> Result<Self, ArtifactError>
    where
        S: AsRef<str>,
    {
        let artifact = serde_json::from_str(json.as_ref())?;
        Ok(artifact)
    }

    /// Loads a truffle artifact from disk.
    pub fn load<P>(path: P) -> Result<Self, ArtifactError>
    where
        P: AsRef<Path>,
    {
        let json = File::open(path)?;
        let artifact = serde_json::from_reader(json)?;
        Ok(artifact)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty() {
        if let Err(err) = Contract::from_json("{}") {
            panic!("error parsing empty artifact: {:?}", err);
        }
    }
}
