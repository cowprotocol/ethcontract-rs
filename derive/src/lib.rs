extern crate proc_macro;

use anyhow::{anyhow, Result};
use ethabi::{Function, ParamType};
use ethcontract_common::truffle::Artifact;
use inflector::Inflector;
use proc_macro2::{Literal, TokenStream};
use quote::quote;
use std::borrow::Cow;
use syn::{parse_macro_input, Error as SynError, LitStr};

#[proc_macro]
pub fn contract(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as LitStr);
    expand_contract(input.clone())
        .unwrap_or_else(|e| SynError::new(input.span(), e.to_string()).to_compile_error())
        .into()
}

macro_rules! ident {
    ($name:expr) => {
        proc_macro2::Ident::new($name, proc_macro2::Span::call_site())
    };
}

fn expand_contract(input: LitStr) -> Result<TokenStream> {
    // TODO(nlordell): we need a general strategy for name collision; we could
    //   keep a set of all names in each scope and append things like `_1` to
    //   the end of the ident in case of collision

    // TODO(nlordell): Due to limitation with the proc-macro Span API, we can't
    //   currently get a path the the file where we were called from; therefore,
    //   the path will always be rooted on the cargo manifest directory.
    //   Eventually we can use the `Span::source_file` API to have a better
    //   experience.
    let artifact_path = input.value();

    let artifact = Artifact::load(&artifact_path)?;
    let contract_name = ident!(&artifact.contract_name.to_pascal_case());

    // TODO(nlordell): only generate `deployed` if there is are netowkrs in the
    //   contract artifact.
    // TODO(nlordell): Generate contructor(fn deploy), fallback, events

    let functions = artifact
        .abi
        .functions()
        .map(|function| expand_function(&artifact, function))
        .collect::<Result<Vec<_>>>()?;

    Ok(quote! {
        /// Instance of a contract with a generated type safe API.
        pub struct #contract_name {
            instance: ethcontract::DynInstance,
        }

        impl #contract_name {
            /// Retrieves the truffle artifact used to generate the type safe API
            /// for this contract.
            pub fn artifact() -> &'static ethcontract::truffle::Artifact {
                use ethcontract::foreign::lazy_static;
                use ethcontract::truffle::Artifact;

                lazy_static! {
                    pub static ref ARTIFACT: Artifact = {
                        Artifact::from_json(
                            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", #input)))
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
                web3: &ethcontract::web3::api::Web3<T>,
                address: ethcontract::web3::types::Address,
            ) -> #contract_name
            where
                F: ethcontract::web3::futures::Future<Item = ethcontract::json::Value, Error = ethcontract::web3::Error> + Send + 'static,
                T: ethcontract::web3::Transport<Out = F> + 'static,
            {
                use ethcontract::contract::Instance;
                use ethcontract::transport::DynTransport;
                use ethcontract::web3::api::Web3;

                let transport = DynTransport::new(web3.transport().clone());
                let web3 = Web3::new(transport);
                let abi = #contract_name ::artifact().abi.clone();
                let instance = Instance::at(web3, abi, address);

                #contract_name { instance }
            }

            /// Locates a deployed contract based on the current network ID
            /// reported by the `web3` provider.
            ///
            /// Note that this does not verify that a contract with a maching
            /// `Abi` is actually deployed at the given address.
            pub fn deployed<F, T>(
                web3: &ethcontract::web3::api::Web3<T>,
            ) -> impl std::future::Future<Output = std::result::Result<#contract_name, ethcontract::contract::DeployError>>
            where
                F: ethcontract::web3::futures::Future<Item = ethcontract::json::Value, Error = ethcontract::web3::Error> + Send + 'static,
                T: ethcontract::web3::Transport<Out = F> + 'static,
            {
                // TODO(nlordell): use async/await once 1.39.0 lands

                use futures::future::{self, TryFutureExt};
                use ethcontract::contract::Instance;
                use ethcontract::transport::DynTransport;
                use ethcontract::truffle::Artifact;
                use ethcontract::web3::api::Web3;

                let transport = DynTransport::new(web3.transport().clone());
                let web3 = Web3::new(transport);
                let artifact = { // only clone the pieces we need
                    let artifact = #contract_name ::artifact();
                    Artifact {
                        abi: artifact.abi.clone(),
                        networks: artifact.networks.clone(),
                        ..Artifact::empty()
                    }
                };
                Instance::deployed(web3, artifact)
                    .and_then(move |instance| future::ok(#contract_name { instance }))
            }

            /// Retrieve the undelying instance being used by this contract.
            pub fn instance(&self) -> &ethcontract::DynInstance {
                &self.instance
            }

            /// Returns the contract address being used by this instance.
            pub fn address(&self) -> ethcontract::web3::types::Address {
                self.instance.address()
            }

            #( #functions )*
        }
    })
}

fn expand_function(artifact: &Artifact, function: &Function) -> Result<TokenStream> {
    let name = ident!(&function.name.to_snake_case());
    let name_str = Literal::string(&function.name);

    let doc_str = artifact
        .devdoc
        .methods
        .get(&function.name)
        .or_else(|| artifact.userdoc.methods.get(&function.name))
        .map(String::as_str)
        .unwrap_or("Generated by `ethcontract`");
    let doc = expand_doc(doc_str);

    let input = expand_fn_inputs(function)?;
    let (method, result) = if function.constant {
        let outputs = expand_fn_outputs(function)?;
        (
            quote! { call },
            quote! { ethcontract::contract::CallBuilder<ethcontract::DynTransport, #outputs> },
        )
    } else {
        (
            quote! { send },
            quote! { ethcontract::contract::TransactionBuilder<ethcontract::DynTransport> },
        )
    };
    let arg = expand_fn_call_arg(function);

    Ok(quote! {
        #doc
        pub fn #name(&self #input) -> #result {
            self.instance.#method(#name_str, #arg)
                .expect("generated call")
        }
    })
}

fn fix_input_name<'a>(i: usize, n: &'a str) -> Cow<'a, str> {
    match n {
        "" => format!("p{}", i).into(),
        n => n.into(),
    }
}

fn expand_fn_inputs(function: &Function) -> Result<TokenStream> {
    let params = function
        .inputs
        .iter()
        .enumerate()
        .map(|(i, param)| {
            let name_str = fix_input_name(i, &param.name);
            let name = ident!(&name_str);
            let kind = expand_type(&param.kind)?;
            Ok(quote! { #name : #kind })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(quote! { #( , #params )* })
}

fn expand_fn_call_arg(function: &Function) -> TokenStream {
    let names = function.inputs.iter().enumerate().map(|(i, param)| {
        let name = fix_input_name(i, &param.name);
        ident!(&name)
    });
    quote! { ( #( #names ),* ) }
}

fn expand_fn_outputs(function: &Function) -> Result<TokenStream> {
    match function.outputs.len() {
        0 => Ok(quote! { () }),
        1 => expand_type(&function.outputs[0].kind),
        _ => {
            let types = function
                .outputs
                .iter()
                .map(|param| expand_type(&param.kind))
                .collect::<Result<Vec<_>>>()?;
            Ok(quote! { (#( #types ),*) })
        }
    }
}

fn expand_type(kind: &ParamType) -> Result<TokenStream> {
    match kind {
        ParamType::Address => Ok(quote! { ethcontract::web3::types::Address }),
        ParamType::Bytes => Ok(quote! { ethcontract::web3::types::Bytes }),
        ParamType::Int(n) | ParamType::Uint(n) => match n {
            // TODO(nlordell): for now, not all uint/int types implement the
            //   `Tokenizable` trait, only `u64`, `U128`, and `U256` so we need
            //   to map solidity int/uint types to those; eventually we should
            //   add more implementations to the `web3` crate
            8 | 16 | 32 | 64 => Ok(quote! { u64 }),
            128 => Ok(quote! { ethcontract::web3::types::U128 }),
            256 => Ok(quote! { ethcontract::web3::types::U256 }),
            n => Err(anyhow!("unsupported solidity type int{}", n)),
        },
        ParamType::Bool => Ok(quote! { bool }),
        ParamType::String => Ok(quote! { String }),
        ParamType::Array(t) => {
            let inner = expand_type(t)?;
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
            let inner = expand_type(t)?;
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
