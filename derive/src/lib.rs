extern crate proc_macro;

use ethcontract_runtime::truffle::Artifact;
use proc_macro2::{TokenStream};
use quote::{quote };
use std::error::Error;
use syn::{parse_macro_input, Error as SynError, LitStr};

type DeriveResult<T> = Result<T, Box<dyn Error + 'static>>;

#[proc_macro]
pub fn contract(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let artifact_path = parse_macro_input!(input as LitStr);
    expand_contract(artifact_path)
        .unwrap_or_else(|e| SynError::new(artifact_path.span(), e.to_string()).to_compile_error())
        .into()
}

macro_rules! ident {
    ($name:expr) => {
        proc_macro2::Ident::new($name, proc_macro2::Span::call_site())
    }
}

fn expand_contract(artifact_path: LitStr) -> DeriveResult<TokenStream> {
    let artifact = Artifact::load(artifact_path.value())?;
    let contract_name = ident!(&artifact.contract_name);

    Ok(quote! {
        #[allow(non-camel-case-types)]
        pub struct #contract_name {
            ethcontract:::
        }
        pub fn foo() -> &'static str {
            #artifact_path
        }
    })
}
