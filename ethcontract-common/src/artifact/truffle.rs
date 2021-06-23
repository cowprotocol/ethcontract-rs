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

/// Parse a truffle artifact from JSON string.
pub fn from_string(json: &str) -> Result<SimpleArtifact, ArtifactError> {
    let origin = "<memory>".to_string();
    let contract = serde_json::from_str(json)?;
    Ok(SimpleArtifact::with_origin(origin, contract))
}

/// Loads a truffle artifact from JSON value.
pub fn from_json(value: Value) -> Result<SimpleArtifact, ArtifactError> {
    let origin = "<memory>".to_string();
    let contract = serde_json::from_value(value)?;
    Ok(SimpleArtifact::with_origin(origin, contract))
}

/// Loads a truffle artifact from disk.
pub fn from_file(path: &Path) -> Result<SimpleArtifact, ArtifactError> {
    let origin = path.to_str().unwrap_or("<filesystem>").to_string();
    let json = File::open(path)?;
    let contract = serde_json::from_reader(json)?;
    Ok(SimpleArtifact::with_origin(origin, contract))
}
