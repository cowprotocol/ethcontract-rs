extern crate proc_macro;

use anyhow::Result;
use ethcontract_common::truffle::Artifact;
use proc_macro2::TokenStream;
use quote::quote;
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
    // TODO(nlordell): Due to limitation with the proc-macro Span API, we can't
    //   currently get a path the the file where we were called from; therefore,
    //   the path will always be rooted on the cargo manifest directory.
    //   Eventually we can use the `Span::source_file` API to have a better
    //   experience.
    let artifact_path = input.value();

    let artifact = Artifact::load(&artifact_path)?;
    let contract_name = ident!(&artifact.contract_name);

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
                eth: ethcontract::web3::api::Eth<T>,
                address: ethcontract::web3::types::Address,
            ) -> #contract_name
            where
                F: ethcontract::web3::futures::Future<Item = ethcontract::json::Value, Error = ethcontract::web3::Error> + Send + 'static,
                T: ethcontract::web3::Transport<Out = F> + 'static,
            {
                use ethcontract::contract::Instance;
                use ethcontract::transport::DynTransport;
                use ethcontract::web3::api::{Eth, Namespace};

                let transport = DynTransport::new(eth.transport().clone());
                let eth = Eth::new(transport);
                let abi = #contract_name ::artifact().abi.clone();
                let instance = Instance::at(eth, abi, address);

                #contract_name { instance }
            }

            /// Locates a deployed contract based on the current network ID
            /// reported by the `web3` provider.
            ///
            /// Note that this does not verify that a contract with a maching
            /// `Abi` is actually deployed at the given address.
            pub async fn deployed<F, T>(
                eth: ethcontract::web3::api::Eth<T>,
            ) -> std::result::Result<#contract_name, ethcontract::contract::DeployedError>
            where
                F: ethcontract::web3::futures::Future<Item = ethcontract::json::Value, Error = ethcontract::web3::Error> + Send + 'static,
                T: ethcontract::web3::Transport<Out = F> + 'static,
            {
                use ethcontract::contract::Instance;
                use ethcontract::transport::DynTransport;
                use ethcontract::web3::api::{Eth, Namespace};

                let transport = DynTransport::new(eth.transport().clone());
                let eth = Eth::new(transport);
                let artifact = #contract_name ::artifact().clone();
                let instance = Instance::deployed(eth, artifact).await?;

                Ok(#contract_name { instance })
            }

            /// Retrieve the undelying `DynInstance` being used by this contract.
            pub fn instance(&self) -> &ethcontract::DynInstance {
                &self.instance
            }
        }
    })
}
