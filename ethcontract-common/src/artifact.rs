//! Tools for loading artifacts that contain compiled contracts.
//!
//! Artifacts come in various shapes and sizes, but usually they
//! are JSON files containing one or multiple compiled contracts
//! as well as their deployment information.
//!
//! This module provides trait [`Artifact`] that encapsulates different
//! artifact models. It also provides tools to load artifacts from different
//! sources, and parse them using different formats.

use crate::contract::{Documentation, Interface, Network};
use crate::{Abi, Bytecode, Contract};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;

pub mod hardhat;
pub mod truffle;

/// An entity that contains compiled contracts.
pub struct Artifact {
    origin: String,
    contracts: HashMap<String, Contract>,
}

impl Artifact {
    /// Creates a new empty artifact.
    pub fn new() -> Self {
        Artifact {
            origin: "<unknown>".to_string(),
            contracts: HashMap::new(),
        }
    }

    /// Creates a new artifact with an origin information.
    pub fn with_origin(origin: impl Into<String>) -> Self {
        Artifact {
            origin: origin.into(),
            contracts: HashMap::new(),
        }
    }

    /// Provides description of where this artifact comes from.
    ///
    /// This function is used when a human-readable reference to the artifact
    /// is required. It could be anything: path to a json file, url, etc.
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Sets new origin for the artifact.
    ///
    /// Artifact loaders will set origin to something meaningful in most cases,
    /// so this function should not be used often. There are cases when
    /// it is required, though.
    pub fn set_origin(&mut self, origin: impl Into<String>) {
        self.origin = origin.into();
    }

    /// Gets number of contracts contained in this artifact.
    pub fn len(&self) -> usize {
        self.contracts.len()
    }

    /// Returns `true` if this artifact contains no contracts.
    pub fn is_empty(&self) -> bool {
        self.contracts.is_empty()
    }

    /// Returns `true` if this artifact has a contract with the given name.
    pub fn contains(&self, name: &str) -> bool {
        self.contracts.contains_key(name)
    }

    /// Looks up contract by its name and returns a reference to it.
    ///
    /// Some artifact formats allow exporting a single unnamed contract.
    /// In this case, the contract will have an empty string as its name.
    pub fn get(&self, name: &str) -> Option<&Contract> {
        self.contracts.get(name)
    }

    /// Looks up contract by its name and returns a handle that allows
    /// safely mutating it.
    ///
    /// The returned handle does not allow renaming contract. For that,
    /// you'll need to remove it and add again.
    pub fn get_mut(&mut self, name: &str) -> Option<ContractMut> {
        self.contracts.get_mut(name).map(ContractMut)
    }

    /// Inserts a new contract to the artifact.
    ///
    /// If contract with this name already exists, replaces it
    /// and returns the old contract.
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

    /// Removes contract from the artifact.
    ///
    /// Returns removed contract or [`None`] if contract with the given name
    /// wasn't found.
    pub fn remove(&mut self, name: &str) -> Option<Contract> {
        self.contracts.remove(name)
    }

    /// Creates an iterator that yields the artifact's contracts.
    pub fn iter(&self) -> impl Iterator<Item = &Contract> + '_ {
        self.contracts.values()
    }

    /// Takes all contracts from the artifact, leaving it empty,
    /// and returns an iterator over the taken contracts.
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

impl ContractMut<'_> {
    /// Returns mutable reference to contract's abi.
    pub fn abi_mut(&mut self) -> &mut Abi {
        &mut Arc::make_mut(&mut self.0.interface).abi
    }

    /// Returns mutable reference to contract's bytecode.
    pub fn bytecode_mut(&mut self) -> &mut Bytecode {
        &mut self.0.bytecode
    }

    /// Returns mutable reference to contract's bytecode.
    pub fn deployed_bytecode_mut(&mut self) -> &mut Bytecode {
        &mut self.0.deployed_bytecode
    }

    /// Returns mutable reference to contract's networks.
    pub fn networks_mut(&mut self) -> &mut HashMap<String, Network> {
        &mut self.0.networks
    }

    /// Returns mutable reference to contract's devdoc.
    pub fn devdoc_mut(&mut self) -> &mut Documentation {
        &mut self.0.devdoc
    }

    /// Returns mutable reference to contract's userdoc.
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

impl Drop for ContractMut<'_> {
    fn drop(&mut self) {
        // The ABI might have gotten mutated while this guard was alive.
        // Since we compute pre-compute and cache a few values based on the ABI
        // as a performance optimization we need to recompute those cached values
        // with the new ABI once the user is done updating the mutable contract.
        let abi = self.0.interface.abi.clone();
        let interface = Interface::from(abi);
        *Arc::make_mut(&mut self.0.interface) = interface;
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn make_contract(name: &str) -> Contract {
        let mut contract = Contract::empty();
        contract.name = name.to_string();
        contract
    }

    #[test]
    fn insert() {
        let mut artifact = Artifact::new();

        assert_eq!(artifact.len(), 0);

        {
            let insert_res = artifact.insert(make_contract("C1"));

            assert_eq!(insert_res.inserted_contract.name, "C1");
            assert!(insert_res.old_contract.is_none());
        }

        assert_eq!(artifact.len(), 1);
        assert!(artifact.contains("C1"));

        {
            let insert_res = artifact.insert(make_contract("C2"));

            assert_eq!(insert_res.inserted_contract.name, "C2");
            assert!(insert_res.old_contract.is_none());
        }

        assert_eq!(artifact.len(), 2);
        assert!(artifact.contains("C2"));

        {
            let insert_res = artifact.insert(make_contract("C1"));

            assert_eq!(insert_res.inserted_contract.name, "C1");
            assert!(insert_res.old_contract.is_some());
        }

        assert_eq!(artifact.len(), 2);
    }

    #[test]
    fn remove() {
        let mut artifact = Artifact::new();

        artifact.insert(make_contract("C1"));
        artifact.insert(make_contract("C2"));

        assert_eq!(artifact.len(), 2);
        assert!(artifact.contains("C1"));
        assert!(artifact.contains("C2"));

        let c0 = artifact.remove("C0");
        assert!(c0.is_none());

        assert_eq!(artifact.len(), 2);
        assert!(artifact.contains("C1"));
        assert!(artifact.contains("C2"));

        let c1 = artifact.remove("C1");

        assert!(c1.is_some());
        assert_eq!(c1.unwrap().name, "C1");

        assert_eq!(artifact.len(), 1);
        assert!(!artifact.contains("C1"));
        assert!(artifact.contains("C2"));
    }
}
