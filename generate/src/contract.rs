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
use anyhow::{Context as _, Result};
use ethcontract_common::truffle::Artifact;
use proc_macro2::{Ident, Literal, TokenStream};
use quote::quote;
use std::env;
use std::fs;
use std::path::PathBuf;

/// Internal shared context for generating smart contract bindings.
pub(crate) struct Context {
    /// The full path to the artifact JSON file.
    full_path: PathBuf,
    /// The artifact path as string literal.
    artifact_path: Literal,
    /// The parsed artifact.
    artifact: Artifact,
    /// The identifier for the runtime crate. Usually this is `ethcontract` but
    /// it can be different if the crate was renamed in the Cargo manifest for
    /// example.
    runtime_crate: Ident,
    /// The contract name as an identifier.
    contract_name: Ident,
    /// The original args used for creating the context.
    args: Args,
}

impl Context {
    /// Create a context from the code generation arguments.
    fn from_args(args: Args) -> Result<Self> {
        let full_path = fs::canonicalize(&args.artifact_path).with_context(|| {
            format!(
                "unable to open file from working dir {} with path {}",
                env::current_dir()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|err| format!("??? ({})", err)),
                args.artifact_path.display(),
            )
        })?;
        let artifact_path = Literal::string(&full_path.to_string_lossy());

        let artifact = Artifact::load(&full_path)
            .with_context(|| format!("failed to parse JSON from file {}", full_path.display()))?;

        let runtime_crate = util::ident(&args.runtime_crate_name);
        let contract_name = util::ident(&artifact.contract_name);

        Ok(Context {
            full_path,
            artifact_path,
            artifact,
            runtime_crate,
            contract_name,
            args,
        })
    }

    #[cfg(test)]
    fn empty() -> Self {
        Context {
            full_path: PathBuf::new(),
            artifact_path: Literal::string(""),
            artifact: Artifact::empty(),
            runtime_crate: util::ident("ethcontract"),
            contract_name: util::ident("Contract"),
            args: Args::new(""),
        }
    }
}

pub(crate) fn expand(args: Args) -> Result<TokenStream> {
    let cx = Context::from_args(args)?;
    let contract = expand_contract(&cx).with_context(|| {
        format!(
            "error expanding contract from JSON {}",
            cx.full_path.display()
        )
    })?;

    Ok(contract)
}

fn expand_contract(cx: &Context) -> Result<TokenStream> {
    let common = common::expand(&cx);
    let deployment = deployment::expand(&cx)?;
    let methods = methods::expand(&cx)?;

    Ok(quote! {
        #common
        #deployment
        #methods
    })
}
