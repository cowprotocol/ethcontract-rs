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
use anyhow::Result;
use ethcontract_common::truffle::Artifact;
use proc_macro2::{Ident, Literal, TokenStream};
use quote::quote;
use std::fs;

/// Internal shared context for generating smart contract bindings.
pub(crate) struct Context {
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
}

impl Context {
    /// Create a context from the code generation arguments.
    fn from_args(args: &Args) -> Result<Self> {
        let full_path = fs::canonicalize(&args.artifact_path)?;
        let artifact_path = Literal::string(&full_path.to_string_lossy());

        let artifact = Artifact::load(&full_path)?;

        let runtime_crate = util::ident(&args.runtime_crate_name);
        let contract_name = util::ident(&artifact.contract_name);

        Ok(Context {
            artifact_path,
            artifact,
            runtime_crate,
            contract_name,
        })
    }
}

pub(crate) fn expand(args: &Args) -> Result<TokenStream> {
    let cx = Context::from_args(args)?;

    let common = common::expand(&cx);
    let deployment = deployment::expand(&cx)?;
    let methods = methods::expand(&cx)?;

    Ok(quote! {
        #common
        #deployment
        #methods
    })
}
