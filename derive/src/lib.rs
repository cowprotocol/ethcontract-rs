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
        pub struct #contract_name <T: ethcontract::web3::Transport> {
            instance: ethcontract::Instance<T>,
        }

        impl<T: ethcontract::web3::Transport> #contract_name <T> {
            pub fn artifact() -> &'static ethcontract::truffle::Artifact {
                ethcontract::foreign::lazy_static! {
                    pub static ref ARTIFACT: ethcontract::truffle::Artifact = {
                        ethcontract::truffle::Artifact::from_json(
                            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", #input)))
                            .expect("valid artifact JSON")
                    };
                }
                &ARTIFACT
            }

            pub fn at(
                eth: ethcontract::web3::api::Eth<T>,
                at: ethcontract::web3::types::Address,
            ) -> #contract_name <T> {
                unimplemented!()
            }
            
            pub fn instance(&self) -> &ethcontract::Instance<T> {
                &self.instance
            }
        }
    })
}
