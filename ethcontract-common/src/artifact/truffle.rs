//! Implements the most common artifact format used in Truffle, Waffle
//! and some other libraries.
//!
//! This artifact is represented as a JSON file containing information about
//! a single contract. We parse the following fields:
//!
//! - `contractName`: name of the contract (optional);
//! - `abi`: information about contract's interface;
//! - `bytecode`: contract's compiled bytecode (optional);
//! - `networks`: info about known contract deployments (optional);
//! - `devdoc`, `userdoc`: additional documentation for contract's methods.

use crate::artifact::Artifact;
use crate::errors::ArtifactError;
use crate::Contract;
use serde_json::{from_reader, from_slice, from_str, from_value, to_string, Value};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

/// Loads truffle artifacts.
pub struct TruffleLoader {
    /// Override for artifact's origin.
    ///
    /// If empty, origin will be derived automatically.
    pub origin: Option<String>,

    /// Override for contract's name.
    ///
    /// Truffle artifacts contain a single contract which may be unnamed.
    pub name: Option<String>,
}

impl TruffleLoader {
    /// Create a new truffle loader.
    pub fn new() -> Self {
        TruffleLoader {
            origin: None,
            name: None,
        }
    }

    /// Create a new truffle loader and set an override for artifact's origins.
    pub fn with_origin(origin: impl Into<String>) -> Self {
        TruffleLoader {
            origin: Some(origin.into()),
            name: None,
        }
    }

    /// Set new override for artifact's origin. See [`origin`] for more info.
    ///
    /// [`origin`]: #structfield.origin
    pub fn origin(mut self, origin: impl Into<String>) -> Self {
        self.origin = Some(origin.into());
        self
    }

    /// Set new override for artifact's name. See [`name`] for more info.
    ///
    /// [`name`]: #structfield.name
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Loads an artifact from a loaded JSON value.
    pub fn load_from_reader(&self, v: impl Read) -> Result<Artifact, ArtifactError> {
        self.load_artifact("<unknown>", v, from_reader)
    }

    /// Loads an artifact from bytes of JSON text.
    pub fn load_from_slice(&self, v: &[u8]) -> Result<Artifact, ArtifactError> {
        self.load_artifact("<unknown>", v, from_slice)
    }

    /// Loads an artifact from string of JSON text.
    pub fn load_from_str(&self, v: &str) -> Result<Artifact, ArtifactError> {
        self.load_artifact("<unknown>", v, from_str)
    }

    /// Loads an artifact from a loaded JSON value.
    pub fn load_from_value(&self, v: Value) -> Result<Artifact, ArtifactError> {
        self.load_artifact("<unknown>", v, from_value)
    }

    /// Loads an artifact from disk.
    pub fn load_from_file(&self, p: impl AsRef<Path>) -> Result<Artifact, ArtifactError> {
        let path = p.as_ref();
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        self.load_artifact(path.display(), reader, from_reader)
    }

    /// Loads a contract from a loaded JSON value.
    pub fn load_contract_from_reader(&self, v: impl Read) -> Result<Contract, ArtifactError> {
        self.load_contract(v, from_reader)
    }

    /// Loads a contract from bytes of JSON text.
    pub fn load_contract_from_slice(&self, v: &[u8]) -> Result<Contract, ArtifactError> {
        self.load_contract(v, from_slice)
    }

    /// Loads a contract from string of JSON text.
    pub fn load_contract_from_str(&self, v: &str) -> Result<Contract, ArtifactError> {
        self.load_contract(v, from_str)
    }

    /// Loads a contract from a loaded JSON value.
    pub fn load_contract_from_value(&self, v: Value) -> Result<Contract, ArtifactError> {
        self.load_contract(v, from_value)
    }

    /// Loads a contract from disk.
    pub fn load_contract_from_file(&self, p: impl AsRef<Path>) -> Result<Contract, ArtifactError> {
        let path = p.as_ref();
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        self.load_contract(reader, from_reader)
    }

    fn load_artifact<T>(
        &self,
        origin: impl ToString,
        source: T,
        loader: impl FnOnce(T) -> serde_json::Result<Contract>,
    ) -> Result<Artifact, ArtifactError> {
        let origin = self.origin.clone().unwrap_or_else(|| origin.to_string());
        let mut artifact = Artifact::with_origin(origin);
        artifact.insert(self.load_contract(source, loader)?);
        Ok(artifact)
    }

    fn load_contract<T>(
        &self,
        source: T,
        loader: impl FnOnce(T) -> serde_json::Result<Contract>,
    ) -> Result<Contract, ArtifactError> {
        let mut contract: Contract = loader(source)?;

        if let Some(name) = &self.name {
            contract.name = name.clone();
        }

        Ok(contract)
    }

    /// Serialize a single contract.
    pub fn save_to_string(contract: &Contract) -> Result<String, ArtifactError> {
        to_string(contract).map_err(Into::into)
    }
}

impl Default for TruffleLoader {
    fn default() -> Self {
        TruffleLoader::new()
    }
}
