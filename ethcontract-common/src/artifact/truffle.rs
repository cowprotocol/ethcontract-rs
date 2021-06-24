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

use crate::artifact::SimpleArtifact;
use crate::errors::ArtifactError;
use serde_json::Value;
use std::fs::File;
use std::path::Path;

/// Loads truffle artifacts.
pub struct TruffleLoader {
    /// Override for artifact's origin. If `None`, origin
    /// will be derived automatically.
    pub origin: Option<String>,
}

impl TruffleLoader {
    /// Create a new truffle loader.
    pub fn new() -> Self {
        TruffleLoader { origin: None }
    }

    /// Create a new truffle loader and set an override for artifact's origins.
    pub fn with_origin(origin: String) -> Self {
        TruffleLoader {
            origin: Some(origin),
        }
    }

    /// Set new override for artifact's origin. See [`origin`] for more info.
    ///
    /// [`origin`]: #structfield.origin
    #[inline]
    pub fn origin(mut self, origin: Option<String>) -> Self {
        self.origin = origin;
        self
    }

    /// Parse a truffle artifact from JSON string.
    pub fn load_from_string(&self, json: &str) -> Result<SimpleArtifact, ArtifactError> {
        let origin = self.origin.as_deref().unwrap_or("<memory>");
        let contract = serde_json::from_str(json)?;
        Ok(SimpleArtifact::with_origin(origin.to_string(), contract))
    }

    /// Loads a truffle artifact from JSON value.
    pub fn load_from_json(&self, value: Value) -> Result<SimpleArtifact, ArtifactError> {
        let origin = self.origin.as_deref().unwrap_or("<memory>");
        let contract = serde_json::from_value(value)?;
        Ok(SimpleArtifact::with_origin(origin.to_string(), contract))
    }

    /// Loads a truffle artifact from disk.
    pub fn load_from_file(&self, path: &Path) -> Result<SimpleArtifact, ArtifactError> {
        let origin = self
            .origin
            .clone()
            .unwrap_or_else(|| format!("{}", path.display()));
        let json = File::open(path)?;
        let contract = serde_json::from_reader(json)?;
        Ok(SimpleArtifact::with_origin(origin, contract))
    }
}

impl Default for TruffleLoader {
    fn default() -> Self {
        TruffleLoader::new()
    }
}
