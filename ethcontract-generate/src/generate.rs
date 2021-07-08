//! Crate for generating type-safe bindings to Ethereum smart contracts. This
//! crate is intended to be used either indirectly with the `ethcontract`
//! crate's `contract` procedural macro or directly from a build script.

mod common;
mod deployment;
mod events;
mod methods;
mod types;

use crate::{util, ContractBuilder};
use anyhow::{anyhow, Context as _, Result};
use ethcontract_common::contract::Network;
use ethcontract_common::Contract;
use inflector::Inflector;
use proc_macro2::{Ident, TokenStream};
use quote::quote;
use std::collections::HashMap;
use syn::{Path, Visibility};

/// Internal shared context for generating smart contract bindings.
pub(crate) struct Context<'a> {
    /// The parsed contract.
    contract: &'a Contract,

    /// The identifier for the runtime crate. Usually this is `ethcontract` but
    /// it can be different if the crate was renamed in the Cargo manifest for
    /// example.
    runtime_crate: Ident,

    /// The visibility for the generated module and re-exported contract type.
    visibility: Visibility,

    /// The name of the module as an identifier in which to place the contract
    /// implementation. Note that the main contract type gets re-exported in the
    /// root.
    contract_mod: Ident,

    /// The contract name as an identifier.
    contract_name: Ident,

    /// Additional contract deployments.
    networks: HashMap<String, Network>,

    /// Manually specified method aliases.
    method_aliases: HashMap<String, Ident>,

    /// Derives added to event structs and enums.
    event_derives: Vec<Path>,
}

impl<'a> Context<'a> {
    /// Creates a context from the code generation arguments.
    fn from_builder(contract: &'a Contract, builder: ContractBuilder) -> Result<Self> {
        let raw_contract_name = if let Some(name) = &builder.contract_name_override {
            name
        } else if !contract.name.is_empty() {
            &contract.name
        } else {
            return Err(anyhow!(
                "contract artifact is missing a name, this can happen when \
                 using a source that does not provide a contract name such as \
                 Etherscan; in this case the contract must be manually \
                 specified"
            ));
        };

        let runtime_crate = util::ident(&builder.runtime_crate_name);
        let visibility = match &builder.visibility_modifier {
            Some(vis) => syn::parse_str(vis)?,
            None => Visibility::Inherited,
        };
        let contract_mod = if let Some(name) = &builder.contract_mod_override {
            util::ident(name)
        } else {
            util::ident(&raw_contract_name.to_snake_case())
        };
        let contract_name = util::ident(raw_contract_name);

        // NOTE: We only check for duplicate signatures here, since if there are
        //   duplicate aliases, the compiler will produce a warning because a
        //   method will be re-defined.
        let mut method_aliases = HashMap::new();
        for (signature, alias) in builder.method_aliases.into_iter() {
            let alias = syn::parse_str(&alias)?;
            if method_aliases.insert(signature.clone(), alias).is_some() {
                return Err(anyhow!(
                    "duplicate method signature '{}' in method aliases",
                    signature,
                ));
            }
        }

        let event_derives = builder
            .event_derives
            .iter()
            .map(|derive| syn::parse_str::<Path>(derive))
            .collect::<Result<Vec<_>, _>>()
            .context("failed to parse event derives")?;

        Ok(Context {
            contract,
            runtime_crate,
            visibility,
            contract_mod,
            contract_name,
            networks: builder.networks,
            method_aliases,
            event_derives,
        })
    }
}

pub(crate) fn expand(contract: &Contract, builder: ContractBuilder) -> Result<TokenStream> {
    let cx = Context::from_builder(contract, builder)?;
    let contract = expand_contract(&cx).context("error expanding contract from its ABI")?;

    Ok(contract)
}

fn expand_contract(cx: &Context) -> Result<TokenStream> {
    let runtime_crate = &cx.runtime_crate;
    let vis = &cx.visibility;
    let contract_mod = &cx.contract_mod;
    let contract_name = &cx.contract_name;

    let common = common::expand(cx);
    let deployment = deployment::expand(cx)?;
    let methods = methods::expand(cx)?;
    let events = events::expand(cx)?;

    Ok(quote! {
        #[allow(dead_code)]
        #vis mod #contract_mod {
            #[rustfmt::skip]
            use #runtime_crate as ethcontract;

            #common
            #deployment
            #methods
            #events
        }
        #vis use self::#contract_mod::Contract as #contract_name;
    })
}
