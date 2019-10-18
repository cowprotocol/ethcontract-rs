//! Module for reading and examining data produced by truffle.

use ethereum_types::Address;
use serde::Deserialize;
use serde_json::Error as JsonError;
use std::collections::HashMap;
use std::fs::File;
use std::io::Error as IoError;
use std::path::Path;
use thiserror::Error;

pub use ethabi::Contract as Abi;

/// Represents a truffle artifact.
#[derive(Clone, Debug, Deserialize)]
pub struct Artifact {
    /// The contract name
    #[serde(rename = "contractName")]
    pub contract_name: String,
    /// The contract ABI
    pub abi: Abi,
    /// The configured networks by network ID for the contract.
    pub networks: HashMap<String, Network>,
}

impl Artifact {
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
