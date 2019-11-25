#![deny(missing_docs)]

//! Implementation of procedural macro for generating type-safe bindings to an
//! ethereum smart contract.

extern crate proc_macro;

use ethcontract_generate::Builder;
use proc_macro::TokenStream;
use syn::ext::IdentExt;
use syn::parse::{Error as ParseError, Parse, ParseStream, Result as ParseResult};
use syn::{parse_macro_input, Error as SynError, Ident, LitStr, Token};

/// Proc macro to generate type-safe bindings to a contract. See
/// [`ethcontract`](ethcontract) module level documentation for more information.
#[proc_macro]
pub fn contract(input: proc_macro::TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as ContractArgs);
    let artifact_span = args.artifact_path.span();
    args.builder
        .generate()
        .map(|bindings| bindings.into_tokens())
        .unwrap_or_else(|e| SynError::new(artifact_span, e.to_string()).to_compile_error())
        .into()
}

/// Contract procedural macro arguments.
struct ContractArgs {
    artifact_path: LitStr,
    builder: Builder,
}

// TODO(nlordell): Due to limitation with the proc-macro Span API, we can't
//   currently get a path the the file where we were called from; therefore,
//   the path will always be rooted on the cargo manifest directory.
//   Eventually we can use the `Span::source_file` API to have a better
//   experience.

impl Parse for ContractArgs {
    fn parse(input: ParseStream) -> ParseResult<Self> {
        let artifact_path: LitStr = input.parse()?;
        let mut builder = Builder::new(artifact_path.value());

        while !input.is_empty() {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                // allow trailing commas
                break;
            }

            let param = input.call(Ident::parse_any)?;
            input.parse::<Token![=]>()?;

            match param.to_string().as_str() {
                "crate" => {
                    let ident = input.call(Ident::parse_any)?;
                    let name = format!("{}", ident.unraw());
                    builder = builder.with_runtime_crate_name(&name);
                }
                _ => {
                    return Err(ParseError::new(
                        param.span(),
                        format!("unexpected named parameter `{}`", param),
                    ))
                }
            }
        }

        Ok(ContractArgs {
            artifact_path,
            builder,
        })
    }
}
