//! Tools for loading artifacts that contain compiled contracts.
//!
//! Artifacts come in various shapes and sizes, but usually they
//! are JSON files containing one or multiple compiled contracts
//! as well as their deployment information.
//!
//! This module provides trait [`Artifact`] that encapsulates different
//! artifact models. It also provides tools to load artifacts from different
//! sources, and parse them using different formats.

use crate::Contract;
use std::collections::HashMap;

pub mod truffle;

/// An entity that contains compiled contracts.
pub struct Artifact {
    origin: String,
    contracts: HashMap<String, Contract>,
}

impl Artifact {
    /// Create a new empty artifact.
    pub fn new() -> Self {
        Artifact {
            origin: "<unknown>".to_string(),
            contracts: HashMap::new(),
        }
    }

    /// Create a new artifact with an origin information.
    pub fn with_origin(origin: String) -> Self {
        Artifact {
            origin,
            contracts: HashMap::new(),
        }
    }

    /// Describe where this artifact comes from.
    ///
    /// This function is used when a human-readable reference to the artifact
    /// is required. It could be anything: path to a json file, url, etc.
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Set new origin for the artifact.
    ///
    /// Artifact loaders will set origin to something meaningful in most cases,
    /// so this function should not be used often. There are cases when
    /// it is required, though.
    pub fn set_origin(&mut self, origin: impl Into<String>) {
        self.origin = origin.into();
    }

    /// Check whether this artifact has a contract with the given name.
    pub fn contains(&self, name: &str) -> bool {
        self.contracts.contains_key(name)
    }

    /// Get contract by name.
    ///
    /// Some artifact formats allow exporting a single unnamed contract.
    /// In this case, the contract will have an empty string as its name.
    pub fn get(&self, name: &str) -> Option<&Contract> {
        self.contracts.get(name)
    }

    /// Insert a new contract to the artifact.
    ///
    /// If contract with this name already exists, replace it
    /// and return an old contract.
    pub fn insert(&mut self, contract: Contract) -> Option<Contract> {
        self.contracts.insert(contract.name.clone(), contract)
    }

    /// Remove contract from the artifact.
    ///
    /// Returns removed contract or [`None`] if contract with the given name
    /// wasn't found.
    pub fn remove(&mut self, name: &str) -> Option<Contract> {
        self.contracts.remove(name)
    }

    /// Create an iterator that yields the artifact's contracts.
    pub fn iter(&self) -> impl Iterator<Item = &Contract> + '_ {
        self.contracts.values()
    }

    /// Take all contracts from the artifact, leaving it empty,
    /// and iterate over them.
    pub fn drain(&mut self) -> impl Iterator<Item = Contract> + '_ {
        self.contracts.drain().map(|(_, contract)| contract)
    }
}

impl Default for Artifact {
    fn default() -> Self {
        Artifact::new()
    }
}
