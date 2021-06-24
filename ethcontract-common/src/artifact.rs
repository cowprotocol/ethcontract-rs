//! Tools for loading artifacts that contain compiled contracts.
//!
//! Artifacts come in various shapes and sizes, but usually they
//! are JSON files containing one or multiple compiled contracts
//! as well as their deployment information.
//!
//! This module provides trait [`Artifact`] that encapsulates different
//! artifact models. It also provides tools to load artifacts from different
//! sources, and parse them using different formats.

use crate::contract::{Documentation, Network};
use crate::{Abi, Bytecode, Contract};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::ops::Deref;

pub mod hardhat;
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
    pub fn with_origin(origin: impl Into<String>) -> Self {
        Artifact {
            origin: origin.into(),
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

    /// Get contract by name.
    ///
    /// Returns a handle that allows mutating the contract. It does not allow
    /// renaming contract though. For that, you'll need to remove
    /// it and add again.
    pub fn get_mut(&mut self, name: &str) -> Option<ContractMut> {
        self.contracts.get_mut(name).map(ContractMut)
    }

    /// Insert a new contract to the artifact.
    ///
    /// If contract with this name already exists, replace it
    /// and return the old contract.
    pub fn insert(&mut self, contract: Contract) -> InsertResult {
        match self.contracts.entry(contract.name.clone()) {
            Entry::Occupied(mut o) => {
                let old_contract = o.insert(contract);
                InsertResult {
                    inserted_contract: ContractMut(o.into_mut()),
                    old_contract: Some(old_contract),
                }
            }
            Entry::Vacant(v) => InsertResult {
                inserted_contract: ContractMut(v.insert(contract)),
                old_contract: None,
            },
        }
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

/// Result of inserting a nre contract into an artifact.
pub struct InsertResult<'a> {
    /// Reference to the newly inserted contract.
    pub inserted_contract: ContractMut<'a>,

    /// If insert operation replaced an old contract, it will appear here.
    pub old_contract: Option<Contract>,
}

/// A wrapper that allows mutating contract
/// but doesn't allow changing its name.
pub struct ContractMut<'a>(&'a mut Contract);

impl<'a> ContractMut<'a> {
    /// Get mutable access to abi.
    pub fn abi_mut(&mut self) -> &mut Abi {
        &mut self.0.abi
    }

    /// Get mutable access to bytecode.
    pub fn bytecode_mut(&mut self) -> &mut Bytecode {
        &mut self.0.bytecode
    }

    /// Get mutable access to networks.
    pub fn networks_mut(&mut self) -> &mut HashMap<String, Network> {
        &mut self.0.networks
    }

    /// Get mutable access to devdoc.
    pub fn devdoc_mut(&mut self) -> &mut Documentation {
        &mut self.0.devdoc
    }

    /// Get mutable access to userdoc.
    pub fn userdoc_mut(&mut self) -> &mut Documentation {
        &mut self.0.userdoc
    }
}

impl Deref for ContractMut<'_> {
    type Target = Contract;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}
