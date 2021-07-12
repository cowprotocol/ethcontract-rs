#![deny(missing_docs, unsafe_code)]

//! Crate for generating type-safe bindings to Ethereum smart contracts. This
//! crate is intended to be used either indirectly with the `ethcontract`
//! crate's `contract` procedural macro or directly from a build script.

#[cfg(test)]
#[allow(missing_docs)]
#[macro_use]
#[path = "test/macros.rs"]
mod test_macros;

pub mod source;

mod generate;
mod rustfmt;
mod util;

pub use crate::source::Source;
pub use crate::util::parse_address;

pub use ethcontract_common::artifact::{Artifact, ContractMut, InsertResult};

/// Convenience re-imports so that you don't have to add `ethcontract-common`
/// as a dependency.
pub mod loaders {
    pub use ethcontract_common::artifact::hardhat::{
        Format as HardHatFormat, HardHatLoader, NetworkEntry,
    };
    pub use ethcontract_common::artifact::truffle::TruffleLoader;
}

use anyhow::Result;
use ethcontract_common::contract::Network;
use ethcontract_common::Contract;
use proc_macro2::TokenStream;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// Builder for generating contract code. Note that no code is generated until
/// the builder is finalized with `generate` or `output`.
pub struct ContractBuilder {
    /// The runtime crate name to use.
    pub runtime_crate_name: String,

    /// The visibility modifier to use for the generated module and contract
    /// re-export.
    pub visibility_modifier: Option<String>,

    /// Override the contract module name that contains the generated code.
    pub contract_mod_override: Option<String>,

    /// Override the contract name to use for the generated type.
    pub contract_name_override: Option<String>,

    /// Manually specified deployed contract address and transaction hash.
    pub networks: HashMap<String, Network>,

    /// Manually specified contract method aliases.
    pub method_aliases: HashMap<String, String>,

    /// Derives added to event structs and enums.
    pub event_derives: Vec<String>,

    /// Format generated code sing locally installed copy of `rustfmt`.
    pub rustfmt: bool,
}

impl ContractBuilder {
    /// Creates a new contract builder with default settings.
    pub fn new() -> Self {
        ContractBuilder {
            runtime_crate_name: "ethcontract".to_string(),
            visibility_modifier: None,
            contract_mod_override: None,
            contract_name_override: None,
            networks: Default::default(),
            method_aliases: Default::default(),
            event_derives: vec![],
            rustfmt: true,
        }
    }

    /// Sets the crate name for the runtime crate. This setting is usually only
    /// needed if the crate was renamed in the Cargo manifest.
    pub fn runtime_crate_name(mut self, name: impl Into<String>) -> Self {
        self.runtime_crate_name = name.into();
        self
    }

    /// Sets an optional visibility modifier for the generated module and
    /// contract re-export.
    pub fn visibility_modifier(mut self, vis: impl Into<String>) -> Self {
        self.visibility_modifier = Some(vis.into());
        self
    }

    /// Sets the optional contract module name override.
    pub fn contract_mod_override(mut self, name: impl Into<String>) -> Self {
        self.contract_mod_override = Some(name.into());
        self
    }

    /// Sets the optional contract name override. This setting is needed when
    /// using an artifact JSON source that does not provide a contract name such
    /// as Etherscan.
    pub fn contract_name_override(mut self, name: impl Into<String>) -> Self {
        self.contract_name_override = Some(name.into());
        self
    }

    /// Adds a deployed address and deployment transaction
    /// hash or block of a contract for a given network. Note that manually
    /// specified deployments take precedence over deployments in the artifact.
    ///
    /// This is useful for integration test scenarios where the address of a
    /// contract on the test node is deterministic, but the contract address
    /// is not in the artifact.
    pub fn add_network(mut self, chain_id: impl Into<String>, network: Network) -> Self {
        self.networks.insert(chain_id.into(), network);
        self
    }

    /// Adds a deployed address. Parses address from string.
    /// See [`add_deployment`] for more information.
    ///
    /// # Panics
    ///
    /// This method panics if the specified address string is invalid. See
    /// [`parse_address`] for more information on the address string format.
    pub fn add_network_str(self, chain_id: impl Into<String>, address: &str) -> Self {
        self.add_network(
            chain_id,
            Network {
                address: parse_address(address).expect("failed to parse address"),
                deployment_information: None,
            },
        )
    }

    /// Adds a solidity method alias to specify what the method name
    /// will be in Rust. For solidity methods without an alias, the snake cased
    /// method name will be used.
    pub fn add_method_alias(
        mut self,
        signature: impl Into<String>,
        alias: impl Into<String>,
    ) -> Self {
        self.method_aliases.insert(signature.into(), alias.into());
        self
    }

    /// Specifies whether or not to format the code using a locally installed
    /// copy of `rustfmt`.
    ///
    /// Note that in case `rustfmt` does not exist or produces an error, the
    /// unformatted code will be used.
    pub fn rustfmt(mut self, rustfmt: bool) -> Self {
        self.rustfmt = rustfmt;
        self
    }

    /// Adds a custom derive to the derives for event structs and enums.
    ///
    /// This makes it possible to, for example, derive `serde::Serialize` and
    /// `serde::Deserialize` for events.
    ///
    /// # Examples
    ///
    /// ```
    /// # use ethcontract_generate::ContractBuilder;
    /// let builder = ContractBuilder::new()
    ///     .add_event_derive("serde::Serialize")
    ///     .add_event_derive("serde::Deserialize");
    /// ```
    pub fn add_event_derive(mut self, derive: impl Into<String>) -> Self {
        self.event_derives.push(derive.into());
        self
    }

    /// Generates the contract bindings.
    pub fn generate(self, contract: &Contract) -> Result<ContractBindings> {
        let rustfmt = self.rustfmt;
        Ok(ContractBindings {
            tokens: generate::expand(contract, self)?,
            rustfmt,
        })
    }
}

impl Default for ContractBuilder {
    fn default() -> Self {
        ContractBuilder::new()
    }
}

/// Type-safe contract bindings generated by a `Builder`. This type can be
/// either written to file or into a token stream for use in a procedural macro.
pub struct ContractBindings {
    /// The TokenStream representing the contract bindings.
    pub tokens: TokenStream,

    /// Format generated code using locally installed copy of `rustfmt`.
    pub rustfmt: bool,
}

impl ContractBindings {
    /// Specifies whether or not to format the code using a locally installed
    /// copy of `rustfmt`.
    ///
    /// Note that in case `rustfmt` does not exist or produces an error, the
    /// unformatted code will be used.
    pub fn rustfmt(mut self, rustfmt: bool) -> Self {
        self.rustfmt = rustfmt;
        self
    }

    /// Writes the bindings to a given `Write`.
    pub fn write(&self, mut w: impl Write) -> Result<()> {
        let source = {
            let raw = self.tokens.to_string();

            if self.rustfmt {
                rustfmt::format(&raw).unwrap_or(raw)
            } else {
                raw
            }
        };

        w.write_all(source.as_bytes())?;
        Ok(())
    }

    /// Writes the bindings to the specified file.
    pub fn write_to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        self.write(writer)
    }

    /// Converts the bindings into its underlying token stream. This allows it
    /// to be used within a procedural macro.
    pub fn into_tokens(self) -> TokenStream {
        self.tokens
    }
}
