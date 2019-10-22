//! Module for reading and examining data produced by truffle.

use serde::Deserialize;
use serde_json::Error as JsonError;
use std::collections::HashMap;
use std::fs::File;
use std::io::Error as IoError;
use std::path::Path;
use thiserror::Error;
use web3::types::{Address, Bytes};

pub use ethabi::Contract as Abi;

/// Represents a truffle artifact.
#[derive(Clone, Debug, Deserialize)]
pub struct Artifact {
    /// The contract name
    #[serde(rename = "contractName")]
    pub contract_name: String,
    /// The contract ABI
    pub abi: Abi,
    /// The bytecode for deploying the contract in its hex representation.
    pub bytecode: Bytes,
    /// The configured networks by network ID for the contract.
    pub networks: HashMap<String, Network>,
    /// The developer documentation.
    pub devdoc: Documentation,
    /// The user documentation.
    pub userdoc: Documentation,
}

impl Artifact {
    /// Creates an empty artifact instance.
    pub fn empty() -> Artifact {
        Artifact {
            contract_name: String::new(),
            abi: Abi {
                constructor: None,
                functions: HashMap::new(),
                events: HashMap::new(),
                fallback: false,
            },
            bytecode: Default::default(),
            networks: HashMap::new(),
            devdoc: Default::default(),
            userdoc: Default::default(),
        }
    }

    /// Parse a truffle artifact from JSON.
    pub fn from_json<S>(json: S) -> Result<Artifact, ArtifactError>
    where
        S: AsRef<str>,
    {
        let artifact = serde_json::from_str(json.as_ref())?;
        Ok(artifact)
    }

    /// Loads a truffle artifact from disk.
    pub fn load<P>(path: P) -> Result<Artifact, ArtifactError>
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
}

/// A contract's documentation.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Documentation {
    /// Contract method documentation.
    pub methods: HashMap<String, String>,
}

/// An error in loading or parsing a truffle artifact.
#[derive(Debug, Error)]
pub enum ArtifactError {
    /// An IO error occurred when loading a truffle artifact from disk.
    #[error("failed to open contract artifact file")]
    Io(#[from] IoError),

    /// A JSON error occurred while parsing a truffle artifact.
    #[error("failed to parse contract artifact JSON")]
    Json(#[from] JsonError),
}
