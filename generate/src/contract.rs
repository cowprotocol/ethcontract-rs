#![deny(missing_docs)]

//! Crate for generating type-safe bindings to Ethereum smart contracts. This
//! crate is intended to be used either indirectly with the `ethcontract`
//! crate's `contract` procedural macro or directly from a build script.

mod common;
mod deployment;
mod methods;
mod types;

use crate::util;
use crate::Args;
use anyhow::{anyhow, Context as _, Result};
use ethcontract_common::{Address, Artifact};
use proc_macro2::{Ident, Literal, TokenStream};
use quote::quote;
use std::collections::HashMap;

/// Internal shared context for generating smart contract bindings.
pub(crate) struct Context {
    /// The artifact JSON as string literal.
    artifact_json: Literal,
    /// The parsed artifact.
    artifact: Artifact,
    /// The identifier for the runtime crate. Usually this is `ethcontract` but
    /// it can be different if the crate was renamed in the Cargo manifest for
    /// example.
    runtime_crate: Ident,
    /// The contract name as an identifier.
    contract_name: Ident,
    /// Additional contract deployments.
    deployments: HashMap<u32, Address>,
}

impl Context {
    /// Create a context from the code generation arguments.
    fn from_args(args: Args) -> Result<Self> {
        let (artifact_json, artifact) = {
            let artifact_json = args
                .artifact_source
                .artifact_json()
                .context("failed to get artifact JSON")?;

            let artifact = Artifact::from_json(&artifact_json)
                .with_context(|| format!("invalid artifact JSON '{}'", artifact_json))
                .with_context(|| {
                    format!(
                        "failed to parse artifact from source {:?}",
                        args.artifact_source,
                    )
                })?;

            (Literal::string(&artifact_json), artifact)
        };

        let runtime_crate = util::ident(&args.runtime_crate_name);
        let contract_name = {
            let name = if let Some(name) = args.contract_name_override.as_ref() {
                name
            } else if !artifact.contract_name.is_empty() {
                &artifact.contract_name
            } else {
                return Err(anyhow!(
                    "contract artifact is missing a name, this can happen when \
                     using a source that does not provide a contract name such \
                     as Etherscan; in this case the contract must be manually \
                     specified"
                ));
            };

            util::ident(name)
        };

        Ok(Context {
            artifact_json,
            artifact,
            runtime_crate,
            contract_name,
            deployments: args.deployments,
        })
    }
}

#[cfg(test)]
impl Default for Context {
    fn default() -> Self {
        Context {
            artifact_json: Literal::string("{}"),
            artifact: Artifact::empty(),
            runtime_crate: util::ident("ethcontract"),
            contract_name: util::ident("Contract"),
            deployments: HashMap::new(),
        }
    }
}

pub(crate) fn expand(args: Args) -> Result<TokenStream> {
    let cx = Context::from_args(args)?;
    let contract = expand_contract(&cx).context("error expanding contract from ABI ")?;

    Ok(contract)
}

fn expand_contract(cx: &Context) -> Result<TokenStream> {
    let common = common::expand(cx);
    let deployment = deployment::expand(cx)?;
    let methods = methods::expand(cx)?;

    Ok(quote! {
        #common
        #deployment
        #methods
    })
}
