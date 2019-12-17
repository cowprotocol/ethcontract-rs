#![deny(missing_docs)]

//! Crate for generating type-safe bindings to Ethereum smart contracts. This
//! crate is intended to be used either indirectly with the `ethcontract`
//! crate's `contract` procedural macro or directly from a build script.

use crate::Args;
use anyhow::{anyhow, Result};
use ethabi::{Function, Param, ParamType};
use ethcontract_common::truffle::Artifact;
use inflector::Inflector;
use proc_macro2::{Ident, Literal, TokenStream};
use quote::quote;
use std::fs;
use syn::Ident as SynIdent;

macro_rules! ident {
    ($name:expr) => {
        Ident::new($name, proc_macro2::Span::call_site())
    };
}

struct Context {
    artifact_path: Literal,
    artifact: Artifact,
    runtime_crate: Ident,
}

impl Context {
    fn from_args(args: &Args) -> Result<Context> {
        let artifact_path = {
            let full_path = fs::canonicalize(&args.artifact_path)?;
            Literal::string(&full_path.to_string_lossy())
        };
        let artifact = Artifact::load(&args.artifact_path)?;
        let runtime_crate = ident!(&args.runtime_crate_name);

        Ok(Context {
            artifact_path,
            artifact,
            runtime_crate,
        })
    }
}

pub(crate) fn expand_contract(args: &Args) -> Result<TokenStream> {
    let cx = Context::from_args(args)?;

    let ethcontract = &cx.runtime_crate;
    let artifact_path = &cx.artifact_path;

    let doc_str = cx
        .artifact
        .devdoc
        .details
        .as_ref()
        .map(String::as_str)
        .unwrap_or("Generated by `ethcontract`");
    let doc = expand_doc(doc_str);
    let contract_name = ident!(&cx.artifact.contract_name);

    let deployed = expand_deployed(&cx);
    let deploy = expand_deploy(&cx)?;

    let functions = cx
        .artifact
        .abi
        .functions()
        .map(|function| expand_function(&cx, function))
        .collect::<Result<Vec<_>>>()?;

    Ok(quote! {
        #doc
        #[allow(non_camel_case_types)]
        pub struct #contract_name {
            instance: #ethcontract::DynInstance,
        }

        #[allow(dead_code)]
        impl #contract_name {
            /// Retrieves the truffle artifact used to generate the type safe API
            /// for this contract.
            pub fn artifact() -> &'static #ethcontract::truffle::Artifact {
                use #ethcontract::foreign::lazy_static;
                use #ethcontract::truffle::Artifact;

                lazy_static! {
                    pub static ref ARTIFACT: Artifact = {
                        Artifact::from_json(include_str!(#artifact_path))
                            .expect("valid artifact JSON")
                    };
                }
                &ARTIFACT
            }

            /// Creates a new contract instance with the specified `web3`
            /// provider at the given `Address`.
            ///
            /// Note that this does not verify that a contract with a maching
            /// `Abi` is actually deployed at the given address.
            pub fn at<F, T>(
                web3: &#ethcontract::web3::api::Web3<T>,
                address: #ethcontract::web3::types::Address,
            ) -> Self
            where
                F: #ethcontract::web3::futures::Future<Item = #ethcontract::json::Value, Error = #ethcontract::web3::Error> + Send + 'static,
                T: #ethcontract::web3::Transport<Out = F> + 'static,
            {
                use #ethcontract::contract::Instance;
                use #ethcontract::transport::DynTransport;
                use #ethcontract::web3::api::Web3;

                let transport = DynTransport::new(web3.transport().clone());
                let web3 = Web3::new(transport);
                let abi = Self::artifact().abi.clone();
                let instance = Instance::at(web3, abi, address);

                Self{ instance }
            }

            /// Retrieve the undelying instance being used by this contract.
            pub fn instance(&self) -> &#ethcontract::DynInstance {
                &self.instance
            }

            /// Retrieve a mutable reference to the undelying instance being
            /// used by this contract.
            pub fn instance_mut(&self) -> &mut #ethcontract::DynInstance {
                &mut self.instance
            }

            /// Returns the contract address being used by this instance.
            pub fn address(&self) -> #ethcontract::web3::types::Address {
                self.instance.address()
            }

            #deployed

            #deploy

            #( #functions )*
        }

        impl #ethcontract::contract::Deploy<#ethcontract::DynTransport> for #contract_name {
            fn deployed_at(
                web3: #ethcontract::web3::api::Web3<#ethcontract::DynTransport>,
                abi: #ethcontract::truffle::Abi,
                at: #ethcontract::web3::types::Address,
            ) -> Self {
                use #ethcontract::contract::Instance;

                // NOTE(nlordell): we need to make sure that we were deployed
                //   with the correct ABI; luckily Abi implementes PartialEq
                assert_eq!(abi, Self::artifact().abi);

                Self {
                    instance: Instance::at(web3, abi, at),
                }
            }
        }
    })
}

fn expand_deployed(cx: &Context) -> TokenStream {
    if cx.artifact.networks.is_empty() {
        return quote! {};
    }

    let ethcontract = &cx.runtime_crate;

    quote! {
        /// Locates a deployed contract based on the current network ID
        /// reported by the `web3` provider.
        ///
        /// Note that this does not verify that a contract with a maching
        /// `Abi` is actually deployed at the given address.
        pub fn deployed<F, T>(
            web3: &#ethcontract::web3::api::Web3<T>,
        ) -> #ethcontract::contract::DeployedFuture<#ethcontract::DynTransport, Self>
        where
            F: #ethcontract::web3::futures::Future<Item = #ethcontract::json::Value, Error = #ethcontract::web3::Error> + Send + 'static,
            T: #ethcontract::web3::Transport<Out = F> + 'static,
        {
            use #ethcontract::contract::DeployedFuture;
            use #ethcontract::transport::DynTransport;
            use #ethcontract::truffle::Artifact;
            use #ethcontract::web3::api::Web3;

            let transport = DynTransport::new(web3.transport().clone());
            let web3 = Web3::new(transport);
            let artifact = { // only clone the pieces we need
                let artifact = Self::artifact();
                Artifact {
                    abi: artifact.abi.clone(),
                    networks: artifact.networks.clone(),
                    ..Artifact::empty()
                }
            };

            DeployedFuture::from_args(web3, artifact)
        }
    }
}

fn expand_deploy(cx: &Context) -> Result<TokenStream> {
    if cx.artifact.bytecode.is_empty() {
        // do not generate deploy method for contracts that have empty bytecode
        return Ok(quote! {});
    }

    let ethcontract = &cx.runtime_crate;

    // TODO(nlordell): not sure how contructor documentation get generated as I
    //   can't seem to get truffle to output it
    let doc = expand_doc("Generated by `ethcontract`");

    let (input, arg) = match cx.artifact.abi.constructor() {
        Some(contructor) => (
            expand_inputs(cx, &contructor.inputs)?,
            expand_inputs_call_arg(&contructor.inputs),
        ),
        None => (quote! {}, quote! {()}),
    };

    // TODO(nlordell): we don't handle duplicate library names
    let lib_params: Vec<_> = cx
        .artifact
        .bytecode
        .undefined_libraries()
        .map(|name| Param {
            name: name.to_snake_case(),
            kind: ParamType::Address,
        })
        .collect();
    let lib_input = expand_inputs(cx, &lib_params)?;

    let link = if !lib_params.is_empty() {
        let link_libraries = cx
            .artifact
            .bytecode
            .undefined_libraries()
            .zip(lib_params.iter())
            .map(|(name, lib_param)| {
                let name = Literal::string(&name);
                let address = ident!(&lib_param.name);

                quote! {
                    artifact.bytecode.link(#name, #address).expect("valid library");
                }
            });

        quote! {
            let mut artifact = artifact;
            #( #link_libraries )*
        }
    } else {
        quote! {}
    };

    Ok(quote! {
        #doc
        pub fn deploy<F, T>(
            web3: &#ethcontract::web3::api::Web3<T> #lib_input #input ,
        ) -> #ethcontract::DynDeployBuilder<Self>
        where
            F: #ethcontract::web3::futures::Future<Item = #ethcontract::json::Value, Error = #ethcontract::web3::Error> + Send + 'static,
            T: #ethcontract::web3::Transport<Out = F> + 'static,
        {
            use #ethcontract::contract::DeployBuilder;
            use #ethcontract::transport::DynTransport;
            use #ethcontract::truffle::Artifact;
            use #ethcontract::web3::api::Web3;

            let transport = DynTransport::new(web3.transport().clone());
            let web3 = Web3::new(transport);

            let artifact = { // only clone the pieces we need
                let artifact = Self::artifact();
                Artifact {
                    abi: artifact.abi.clone(),
                    bytecode: artifact.bytecode.clone(),
                    ..Artifact::empty()
                }
            };
            #link

            DeployBuilder::new(web3, artifact, #arg).expect("valid deployment args")
        }
    })
}

fn expand_function(cx: &Context, function: &Function) -> Result<TokenStream> {
    let ethcontract = &cx.runtime_crate;

    let name = ident!(&function.name.to_snake_case());
    let name_str = Literal::string(&function.name);

    let signature = function_signature(&function);
    let doc_str = cx
        .artifact
        .devdoc
        .methods
        .get(&signature)
        .or_else(|| cx.artifact.userdoc.methods.get(&signature))
        .and_then(|entry| entry.details.as_ref())
        .map(String::as_str)
        .unwrap_or("Generated by `ethcontract`");
    let doc = expand_doc(doc_str);

    let input = expand_inputs(cx, &function.inputs)?;
    let outputs = expand_fn_outputs(cx, &function)?;
    let (method, result_type_name) = if function.constant {
        (quote! { view_method }, quote! { DynViewMethodBuilder })
    } else {
        (quote! { method }, quote! { DynMethodBuilder })
    };
    let result = quote! { #ethcontract::#result_type_name<#outputs> };
    let arg = expand_inputs_call_arg(&function.inputs);

    Ok(quote! {
        #doc
        pub fn #name(&self #input) -> #result {
            self.instance.#method(#name_str, #arg)
                .expect("generated call")
        }
    })
}

fn function_signature(function: &Function) -> String {
    let types = match function.inputs.len() {
        0 => String::new(),
        _ => {
            let mut params = function.inputs.iter().map(|param| &param.kind);
            let first = params.next().expect("at least one param").to_string();
            params.fold(first, |acc, param| format!("{},{}", acc, param))
        }
    };
    format!("{}({})", function.name, types)
}

fn expand_inputs(cx: &Context, inputs: &[Param]) -> Result<TokenStream> {
    let params = inputs
        .iter()
        .enumerate()
        .map(|(i, param)| {
            let name = expand_input_name(i, &param.name);
            let kind = expand_type(cx, &param.kind)?;
            Ok(quote! { #name: #kind })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(quote! { #( , #params )* })
}

fn expand_input_name(index: usize, name: &str) -> TokenStream {
    let name_str = match name {
        "" => format!("p{}", index),
        n => n.to_snake_case(),
    };
    let name =
        syn::parse_str::<SynIdent>(&name_str).unwrap_or_else(|_| ident!(&format!("{}_", name_str)));

    quote! { #name }
}

fn expand_inputs_call_arg(inputs: &[Param]) -> TokenStream {
    let names = inputs
        .iter()
        .enumerate()
        .map(|(i, param)| expand_input_name(i, &param.name));
    quote! { ( #( #names ,)* ) }
}

fn expand_fn_outputs(cx: &Context, function: &Function) -> Result<TokenStream> {
    match function.outputs.len() {
        0 => Ok(quote! { () }),
        1 => expand_type(cx, &function.outputs[0].kind),
        _ => {
            let types = function
                .outputs
                .iter()
                .map(|param| expand_type(cx, &param.kind))
                .collect::<Result<Vec<_>>>()?;
            Ok(quote! { (#( #types ),*) })
        }
    }
}

fn expand_type(cx: &Context, kind: &ParamType) -> Result<TokenStream> {
    let ethcontract = &cx.runtime_crate;

    match kind {
        ParamType::Address => Ok(quote! { #ethcontract::web3::types::Address }),
        ParamType::Bytes => Ok(quote! { #ethcontract::web3::types::Bytes }),
        ParamType::Int(n) | ParamType::Uint(n) => match n {
            // TODO(nlordell): for now, not all uint/int types implement the
            //   `Tokenizable` trait, only `u64`, `U128`, and `U256` so we need
            //   to map solidity int/uint types to those; eventually we should
            //   add more implementations to the `web3` crate
            8 | 16 | 32 | 64 => Ok(quote! { u64 }),
            128 => Ok(quote! { #ethcontract::web3::types::U128 }),
            256 => Ok(quote! { #ethcontract::web3::types::U256 }),
            n => Err(anyhow!("unsupported solidity type int{}", n)),
        },
        ParamType::Bool => Ok(quote! { bool }),
        ParamType::String => Ok(quote! { String }),
        ParamType::Array(t) => {
            let inner = expand_type(cx, t)?;
            Ok(quote! { Vec<#inner> })
        }
        ParamType::FixedBytes(n) => {
            // TODO(nlordell): what is the performance impact of returning large
            //   `FixedBytes` and `FixedArray`s with `web3`?
            let size = Literal::usize_unsuffixed(*n);
            Ok(quote! { [u8; #size] })
        }
        ParamType::FixedArray(t, n) => {
            // TODO(nlordell): see above
            let inner = expand_type(cx, t)?;
            let size = Literal::usize_unsuffixed(*n);
            Ok(quote! { [#inner; #size] })
        }
    }
}

fn expand_doc(s: &str) -> TokenStream {
    let doc = Literal::string(s);
    quote! {
        #[doc = #doc]
    }
}
