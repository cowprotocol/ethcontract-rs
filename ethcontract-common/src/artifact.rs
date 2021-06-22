//!

use crate::errors::ArtifactError;
use crate::Contract;

/// An entity that contains compiled contracts.
pub trait Artifact {
    /// Describes where this artifact comes from. This could be anything:
    /// path to a json file, url, or something else. This function is used
    /// in error messages.
    fn origin(&self) -> Option<&str>;

    /// Get contract by name. Pass an empty string to get an unnamed contract
    /// if an artifact implementation supports it.
    fn contract(&self, name: &str) -> Result<Option<&Contract>, ArtifactError>;

    /// Iterate over contracts in the artifact. Order of iteration
    /// is not specified.
    fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = &Contract> + 'a>;
}

/// A simple [`Artifact`] implementation that only holds one contract.
///
/// This is used to represent artifacts similar to truffle and waffle.
pub struct SimpleArtifact {
    origin: Option<String>,
    contract: Contract,
}

impl SimpleArtifact {
    /// Create a new artifact by wrapping a contract into it.
    pub fn new(contract: Contract) -> Self {
        SimpleArtifact { origin: None, contract }
    }

    /// Create a new artifact with an origin information.
    pub fn with_origin(origin: String, contract: Contract) -> Self {
        SimpleArtifact { origin: Some(origin), contract }
    }

    /// Get a reference to the artifact's contract.
    pub fn contract(&self) -> &Contract {
        &self.contract
    }

    /// Get a mutable reference to the artifact's contract.
    pub fn contract_mut(&mut self) -> &mut Contract {
        &mut self.contract
    }

    /// Set a new name for the contract.
    pub fn origin(mut self, origin: String) -> Self {
        self.origin = Some(origin);
        self
    }

    /// Set a new name for the contract.
    pub fn name(mut self, name: String) -> Self {
        self.contract.name = name;
        self
    }

    /// Extract contract from the artifact.
    pub fn into_inner(self) -> Contract {
        self.contract
    }
}

impl Artifact for SimpleArtifact {
    fn origin(&self) -> Option<&str> {
        self.origin.as_deref()
    }

    fn contract(&self, name: &str) -> Result<Option<&Contract>, ArtifactError> {
        if name == self.contract.name {
            Ok(Some(&self.contract))
        } else {
            Ok(None)
        }
    }

    fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = &Contract> + 'a> {
        Box::new(SimpleArtifactIter { contract: Some(&self.contract) })
    }
}

struct SimpleArtifactIter<'a> {
    contract: Option<&'a Contract>
}

impl<'a> Iterator for SimpleArtifactIter<'a> {
    type Item = &'a Contract;

    fn next(&mut self) -> Option<Self::Item> {
        self.contract.take()
    }
}
