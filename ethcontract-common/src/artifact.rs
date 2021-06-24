//! Tools for loading artifacts that contain compiled contracts.
//!
//! Artifacts come in various shapes and sizes, but usually they
//! are JSON files containing one or multiple compiled contracts
//! as well as their deployment information.
//!
//! This module provides trait [`Artifact`] that encapsulates different
//! artifact models. It also provides tools to load artifacts from different
//! sources, and parse them using different formats.

use crate::errors::ArtifactError;
use crate::Contract;

pub mod truffle;
pub mod hardhat;

/// An entity that contains compiled contracts.
pub trait Artifact {
    /// Describes where this artifact comes from. This could be anything:
    /// path to a json file, url, or something else. This function is used
    /// in error messages.
    fn origin(&self) -> &str;

    /// Get contract by name. Pass an empty string to get an unnamed contract
    /// if an artifact implementation supports it.
    fn contract(&self, name: &str) -> Result<Option<&Contract>, ArtifactError>;

    /// Iterate over contracts in the artifact. Order of iteration
    /// is not specified.
    fn iter(&self) -> Box<dyn Iterator<Item = &Contract> + '_>;
}

/// A simple [`Artifact`] implementation that only holds one contract.
///
/// This is used to represent artifacts similar to truffle and waffle.
pub struct SimpleArtifact {
    origin: String,
    contract: Contract,
}

impl SimpleArtifact {
    /// Create a new artifact by wrapping a contract into it.
    pub fn new(contract: Contract) -> Self {
        SimpleArtifact {
            origin: "<unknown>".to_string(),
            contract,
        }
    }

    /// Create a new artifact with an origin information.
    pub fn with_origin(origin: String, contract: Contract) -> Self {
        SimpleArtifact {
            origin,
            contract,
        }
    }

    /// Get a reference to the artifact's contract.
    pub fn contract(&self) -> &Contract {
        &self.contract
    }

    /// Get a mutable reference to the artifact's contract.
    pub fn contract_mut(&mut self) -> &mut Contract {
        &mut self.contract
    }

    /// Set new origin for the artifact.
    pub fn set_origin(&mut self, origin: String) {
        self.origin = origin;
    }

    /// Set new contract name.
    pub fn set_name(&mut self, name: String) {
        self.contract.name = name;
    }

    /// Extract contract from the artifact.
    pub fn into_inner(self) -> Contract {
        self.contract
    }
}

impl Artifact for SimpleArtifact {
    fn origin(&self) -> &str {
        &self.origin
    }

    fn contract(&self, name: &str) -> Result<Option<&Contract>, ArtifactError> {
        if name == self.contract.name {
            Ok(Some(&self.contract))
        } else {
            Ok(None)
        }
    }

    fn iter(&self) -> Box<dyn Iterator<Item = &Contract> + '_> {
        Box::new(std::iter::once(&self.contract))
    }
}
