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
use serde_json::Value;
use std::fs::File;
use std::path::Path;
use crate::Contract;

/// Loads truffle artifacts.
pub struct TruffleLoader {
    /// Override for artifact's origin.
    ///
    /// If empty, origin will be derived automatically.
    pub origin: Option<String>,

    /// Override for contract's name.
    ///
    /// Truffle artifacts contain a single contract which may
    pub name: Option<String>,
}

impl TruffleLoader {
    /// Create a new truffle loader.
    pub fn new() -> Self {
        TruffleLoader { origin: None, name: None }
    }

    /// Create a new truffle loader and set an override for artifact's origins.
    pub fn with_origin(origin: String) -> Self {
        TruffleLoader {
            origin: Some(origin),
            name: None
        }
    }

    /// Set new override for artifact's origin. See [`origin`] for more info.
    ///
    /// [`origin`]: #structfield.origin
    #[inline]
    pub fn origin(mut self, origin: String) -> Self {
        self.origin = Some(origin);
        self
    }

    /// Set new override for artifact's name. See [`name`] for more info.
    ///
    /// [`name`]: #structfield.name
    #[inline]
    pub fn name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }

    /// Parse a truffle artifact from JSON string.
    pub fn load_from_string(&self, json: &str) -> Result<Artifact, ArtifactError> {
        let origin = self.origin.clone().unwrap_or_else(|| "<memory>".to_string());
        let mut artifact = Artifact::with_origin(origin);
        artifact.insert(self.load_contract_from_string(json)?);
        Ok(artifact)
    }

    /// Parse a contract from JSON string.
    pub fn load_contract_from_string(&self, json: &str) -> Result<Contract, ArtifactError> {
        let mut contract: Contract = serde_json::from_str(json)?;
        if let Some(name) = &self.name {
            contract.name = name.clone();
        }
        Ok(contract)
    }

    /// Loads a truffle artifact from JSON value.
    pub fn load_from_json(&self, value: Value) -> Result<Artifact, ArtifactError> {
        let origin = self.origin.clone().unwrap_or_else(|| "<memory>".to_string());
        let mut artifact = Artifact::with_origin(origin);
        artifact.insert(self.load_contract_from_json(value)?);
        Ok(artifact)
    }

    /// Loads a contract from JSON value.
    pub fn load_contract_from_json(&self, value: Value) -> Result<Contract, ArtifactError> {
        let mut contract: Contract = serde_json::from_value(value)?;
        if let Some(name) = &self.name {
            contract.name = name.clone();
        }
        Ok(contract)
    }

    /// Loads a truffle artifact from disk.
    pub fn load_from_file(&self, path: &Path) -> Result<Artifact, ArtifactError> {
        let origin = self.origin.clone().unwrap_or_else(|| path.display().to_string());
        let mut artifact = Artifact::with_origin(origin);
        artifact.insert(self.load_contract_from_file(path)?);
        Ok(artifact)
    }

    /// Loads a contract from disk.
    pub fn load_contract_from_file(&self, path: &Path) -> Result<Contract, ArtifactError> {
        let mut contract: Contract = serde_json::from_reader(File::open(path)?)?;
        if let Some(name) = &self.name {
            contract.name = name.clone();
        }
        Ok(contract)
    }
}

impl Default for TruffleLoader {
    fn default() -> Self {
        TruffleLoader::new()
    }
}
