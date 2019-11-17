#![deny(missing_docs)]

//! Implementation of procedural macro for generating type-safe bindings to an
//! ethereum smart contract.

extern crate proc_macro;

use anyhow::{anyhow, Result};
use ethabi::{Function, Param, ParamType};
use ethcontract_common::truffle::Artifact;
use inflector::Inflector;
use proc_macro2::{Ident, Literal, Span, TokenStream};
use quote::quote;
use std::fs;
use syn::ext::IdentExt;
use syn::parse::{Error as ParseError, Parse, ParseStream, Result as ParseResult};
use syn::{parse_macro_input, Error as SynError, Ident as SynIdent, LitStr, Token};

/// Proc macro to generate type-safe bindings to a contract. See
/// [`ethcontract`](ethcontract) module level documentation for more information.
#[proc_macro]
pub fn contract(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let args = parse_macro_input!(input as ContractArgs);
    expand_contract(args)
        .unwrap_or_else(|e| SynError::new(Span::call_site(), e.to_string()).to_compile_error())
        .into()
}

macro_rules! ident {
    ($name:expr) => {
        Ident::new($name, proc_macro2::Span::call_site())
    };
}

struct ContractArgs {
    artifact_path: LitStr,
    runtime_crate: Option<Ident>,
}

impl Parse for ContractArgs {
    fn parse(input: ParseStream) -> ParseResult<Self> {
        let mut result = ContractArgs {
            artifact_path: input.parse()?,
            runtime_crate: None,
        };

        while !input.is_empty() {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }

            let param = input.call(Ident::parse_any)?;
            input.parse::<Token![=]>()?;

            match param.to_string().as_str() {
                "crate" => result.runtime_crate = Some(input.call(Ident::parse_any)?),
                _ => {
                    return Err(ParseError::new(
                        param.span(),
                        format!("unexpected named parameter `{}`", param),
                    ))
                }
            }
        }

        Ok(result)
    }
}

fn expand_contract(args: ContractArgs) -> Result<TokenStream> {
    // TODO(nlordell): we need a general strategy for name collision; we could
    //   keep a set of all names in each scope and append things like `_1` to
    //   the end of the ident in case of collision

    // TODO(nlordell): Due to limitation with the proc-macro Span API, we can't
    //   currently get a path the the file where we were called from; therefore,
    //   the path will always be rooted on the cargo manifest directory.
    //   Eventually we can use the `Span::source_file` API to have a better
    //   experience.
    let artifact_path = {
        let full_path = fs::canonicalize(args.artifact_path.value())?;
        LitStr::new(&full_path.to_string_lossy(), args.artifact_path.span())
    };
    let artifact = Artifact::load(&artifact_path.value())?;
    let contract_name = ident!(&artifact.contract_name);

    let ethcontract = args.runtime_crate.unwrap_or_else(|| ident!("ethcontract"));

    let doc_str = artifact
        .devdoc
        .details
        .as_ref()
        .map(String::as_str)
        .unwrap_or("Generated by `ethcontract`");
    let doc = expand_doc(doc_str);

    let deployed = expand_deployed(&ethcontract, &artifact);
    let deploy = expand_deploy(&ethcontract, &artifact)?;

    // TODO(nlordell): Generate fallback, events

    let functions = artifact
        .abi
        .functions()
        .map(|function| expand_function(&artifact, &ethcontract, function))
        .collect::<Result<Vec<_>>>()?;

    Ok(quote! {
        #doc
        pub struct #contract_name {
            instance: #ethcontract::DynInstance,
        }

        impl #contract_name {
            /// Retrieves the truffle artifact used to generate the type safe API
            /// for this contract.
            pub fn artifact() -> &'static #ethcontract::truffle::Artifact {
                use #ethcontract::foreign::lazy_static;
                use #ethcontract::truffle::Artifact;

                lazy_static! {
                    pub static ref ARTIFACT: Artifact = {
                        Artifact::from_json(#artifact_path).expect("valid artifact JSON")
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

fn expand_deployed(ethcontract: &Ident, artifact: &Artifact) -> TokenStream {
    if artifact.networks.is_empty() {
        return quote! {};
    }

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

fn expand_deploy(ethcontract: &Ident, artifact: &Artifact) -> Result<TokenStream> {
    // TODO(nlordell): not sure how contructor documentation get generated as I
    //   can't seem to get truffle to output it
    let doc = expand_doc("Generated by `ethcontract`");

    let (input, arg) = match artifact.abi.constructor() {
        Some(contructor) => (
            expand_inputs(ethcontract, &contructor.inputs)?,
            expand_inputs_call_arg(&contructor.inputs),
        ),
        None => (quote! {}, quote! {()}),
    };

    // TODO(nlordell): we don't handle duplicate library names
    let libraries: Vec<_> = artifact
        .bytecode
        .undefined_libraries()
        .map(|name| Param {
            name: name.to_snake_case(),
            kind: ParamType::Address,
        })
        .collect();
    let lib_input = expand_inputs(ethcontract, &libraries)?;

    let link = if libraries.is_empty() {
        let link_libraries = libraries.iter().map(|lib| {
            let name = Literal::string(&lib.name);
            let address = ident!(&lib.name);

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
            web3: &#ethcontract::web3::api::Web3<T> #input #lib_input ,
        ) -> #ethcontract::contract::DeployBuilder<#ethcontract::DynTransport, Self>
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

fn expand_function(
    artifact: &Artifact,
    ethcontract: &Ident,
    function: &Function,
) -> Result<TokenStream> {
    let name = ident!(&function.name.to_snake_case());
    let name_str = Literal::string(&function.name);

    let signature = function_signature(&function);
    let doc_str = artifact
        .devdoc
        .methods
        .get(&signature)
        .or_else(|| artifact.userdoc.methods.get(&signature))
        .and_then(|entry| entry.details.as_ref())
        .map(String::as_str)
        .unwrap_or("Generated by `ethcontract`");
    let doc = expand_doc(doc_str);

    let input = expand_inputs(ethcontract, &function.inputs)?;
    let outputs = expand_fn_outputs(ethcontract, &function)?;
    let (method, result_type_name) = if function.constant {
        (quote! { view_method }, quote! { ViewMethodBuilder })
    } else {
        (quote! { method }, quote! { MethodBuilder })
    };
    let result =
        quote! { #ethcontract::contract::#result_type_name<#ethcontract::DynTransport, #outputs> };
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

fn expand_inputs(ethcontract: &Ident, inputs: &[Param]) -> Result<TokenStream> {
    let params = inputs
        .iter()
        .enumerate()
        .map(|(i, param)| {
            let name = expand_input_name(i, &param.name);
            let kind = expand_type(ethcontract, &param.kind)?;
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
    quote! { ( #( #names ),* ) }
}

fn expand_fn_outputs(ethcontract: &Ident, function: &Function) -> Result<TokenStream> {
    match function.outputs.len() {
        0 => Ok(quote! { () }),
        1 => expand_type(ethcontract, &function.outputs[0].kind),
        _ => {
            let types = function
                .outputs
                .iter()
                .map(|param| expand_type(ethcontract, &param.kind))
                .collect::<Result<Vec<_>>>()?;
            Ok(quote! { (#( #types ),*) })
        }
    }
}

fn expand_type(ethcontract: &Ident, kind: &ParamType) -> Result<TokenStream> {
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
            let inner = expand_type(ethcontract, t)?;
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
            let inner = expand_type(ethcontract, t)?;
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
