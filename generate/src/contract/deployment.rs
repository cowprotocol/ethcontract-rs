use crate::contract::{methods, Context};
use crate::util;
use anyhow::{Context as _, Result};
use ethcontract_common::abi::{Param, ParamType};
use inflector::Inflector;
use proc_macro2::{Literal, TokenStream};
use quote::quote;

pub(crate) fn expand(cx: &Context) -> Result<TokenStream> {
    let deployed = expand_deployed(&cx);
    let deploy =
        expand_deploy(&cx).context("error generating contract `deploy` associated function")?;

    Ok(quote! {
        #deployed
        #deploy
    })
}

fn expand_deployed(cx: &Context) -> TokenStream {
    if cx.artifact.networks.is_empty() {
        return quote! {};
    }

    let ethcontract = &cx.runtime_crate;
    let contract_name = &cx.contract_name;

    quote! {
        impl #contract_name {
            /// Locates a deployed contract based on the current network ID
            /// reported by the `web3` provider.
            ///
            /// Note that this does not verify that a contract with a maching
            /// `Abi` is actually deployed at the given address.
            pub fn deployed<F, T>(
                web3: &#ethcontract::web3::api::Web3<T>,
            ) -> #ethcontract::contract::DeployedFuture<#ethcontract::transport::DynTransport, Self>
            where
                F: #ethcontract::web3::futures::Future<
                    Item = #ethcontract::json::Value,
                    Error = #ethcontract::web3::Error
                > + Send + 'static,
                T: #ethcontract::web3::Transport<Out = F> + 'static,
            {
                use #ethcontract::contract::DeployedFuture;
                use #ethcontract::transport::DynTransport;
                use #ethcontract::web3::api::Web3;

                let transport = DynTransport::new(web3.transport().clone());
                let web3 = Web3::new(transport);

                DeployedFuture::new(web3, ())
            }
        }

        impl #ethcontract::contract::FromNetwork<#ethcontract::DynTransport> for #contract_name {
            type Context = ();

            fn from_network(web3: #ethcontract::DynWeb3, network_id: &str, _: Self::Context) -> Option<Self> {
                use #ethcontract::Instance;

                let artifact = Self::artifact();
                artifact
                    .networks
                    .get(network_id)
                    .map(move |network| Self {
                        instance: Instance::at(web3, artifact.abi.clone(), network.address),
                    })
            }
        }
    }
}

fn expand_deploy(cx: &Context) -> Result<TokenStream> {
    if cx.artifact.bytecode.is_empty() {
        // do not generate deploy method for contracts that have empty bytecode
        return Ok(quote! {});
    }

    let ethcontract = &cx.runtime_crate;
    let contract_name = &cx.contract_name;

    // TODO(nlordell): not sure how contructor documentation get generated as I
    //   can't seem to get truffle to output it
    let doc = util::expand_doc("Generated by `ethcontract`");

    let (input, arg) = match cx.artifact.abi.constructor() {
        Some(contructor) => (
            methods::expand_inputs(cx, &contructor.inputs)?,
            methods::expand_inputs_call_arg(&contructor.inputs),
        ),
        None => (quote! {}, quote! {()}),
    };

    let lib_params: Vec<_> = cx
        .artifact
        .bytecode
        .undefined_libraries()
        .map(|name| Param {
            name: name.to_snake_case(),
            kind: ParamType::Address,
        })
        .collect();
    let lib_input = methods::expand_inputs(cx, &lib_params)?;

    let link = if !lib_params.is_empty() {
        let link_libraries = cx
            .artifact
            .bytecode
            .undefined_libraries()
            .zip(lib_params.iter())
            .map(|(name, lib_param)| {
                let name = Literal::string(&name);
                let address = util::ident(&lib_param.name);

                quote! {
                    bytecode.link(#name, #address).expect("valid library");
                }
            });

        quote! {
            let mut bytecode = bytecode;
            #( #link_libraries )*
        }
    } else {
        quote! {}
    };

    Ok(quote! {
        impl #contract_name {
            #doc
            pub fn builder<F, T>(
                web3: &#ethcontract::web3::api::Web3<T> #lib_input #input ,
            ) -> #ethcontract::DynDeployBuilder<Self>
            where
                F: #ethcontract::web3::futures::Future<Item = #ethcontract::json::Value, Error = #ethcontract::web3::Error> + Send + 'static,
                T: #ethcontract::web3::Transport<Out = F> + 'static,
            {
                use #ethcontract::DynTransport;
                use #ethcontract::contract::DeployBuilder;
                use #ethcontract::web3::api::Web3;

                let transport = DynTransport::new(web3.transport().clone());
                let web3 = Web3::new(transport);

                let bytecode = Self::artifact().bytecode.clone();
                #link

                DeployBuilder::new(web3, bytecode, #arg).expect("valid deployment args")
            }
        }

        impl #ethcontract::contract::Deploy<#ethcontract::DynTransport> for #contract_name {
            type Context = #ethcontract::common::Bytecode;

            fn bytecode(cx: &Self::Context) -> &#ethcontract::common::Bytecode {
                cx
            }

            fn abi(_: &Self::Context) -> &#ethcontract::common::Abi {
                &Self::artifact().abi
            }

            fn at_address(web3: #ethcontract::DynWeb3, address: #ethcontract::Address, _: Self::Context) -> Self {
                use #ethcontract::Instance;

                let abi = Self::artifact().abi.clone();
                Self {
                    instance: Instance::at(web3, abi, address),
                }
            }
        }
    })
}
