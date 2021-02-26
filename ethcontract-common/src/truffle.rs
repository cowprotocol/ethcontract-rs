//! Module for reading and examining data produced by truffle.

use crate::abi::Contract as Abi;
use crate::errors::ArtifactError;
use crate::{bytecode::Bytecode, DeploymentInformation};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use web3::types::Address;

/// Represents a truffle artifact.
#[derive(Clone, Debug, Deserialize)]
#[serde(default = "Artifact::empty")]
pub struct Artifact {
    /// The contract name
    #[serde(rename = "contractName")]
    pub contract_name: String,
    /// The contract ABI
    #[serde(with = "compat_solc_v0_6")]
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

impl Artifact {
    /// Creates an empty artifact instance.
    pub fn empty() -> Self {
        Artifact {
            contract_name: String::new(),
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

    /// Parse a truffle artifact from JSON.
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
#[derive(Clone, Debug, Deserialize)]
pub struct Network {
    /// The address at which the contract is deployed on this network.
    pub address: Address,
    /// The hash of the transaction that deployed the contract on this network.
    #[serde(rename = "transactionHash")]
    pub deployment_information: Option<DeploymentInformation>,
}

/// A contract's documentation.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Documentation {
    /// Contract documentation
    pub details: Option<String>,
    /// Contract method documentation.
    pub methods: HashMap<String, DocEntry>,
}

#[derive(Clone, Debug, Default, Deserialize)]
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
        if let Err(err) = Artifact::from_json("{}") {
            panic!("error parsing empty artifact: {:?}", err);
        }
    }
}

/// Deserialization implementation for compatibility with ABIs produced with
/// solc v0.6. This is a known `ethabi` issue with an open PR for a fix:
/// https://github.com/openethereum/ethabi/issues/185
/// https://github.com/openethereum/ethabi/pull/187
mod compat_solc_v0_6 {
    use super::*;
    use serde::de::{self, Deserializer};
    use serde_json::{json, Value};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Abi, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut raw_abi: Vec<Value> = Deserialize::deserialize(deserializer)?;
        let receive = json!("receive");
        for entry in raw_abi.iter_mut() {
            if let Value::Object(obj) = entry {
                if let Some(kind) = obj.get_mut("type") {
                    if kind == &receive {
                        *kind = json!("fallback");
                    }
                }
            }
        }

        Abi::deserialize(Value::Array(raw_abi)).map_err(de::Error::custom)
    }
}
