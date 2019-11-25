#![deny(missing_docs)]

//! Implementation of procedural macro for generating type-safe bindings to an
//! ethereum smart contract.

extern crate proc_macro;

use syn::ext::IdentExt;
use syn::parse::{Error as ParseError, Parse, ParseStream, Result as ParseResult};

/// Proc macro to generate type-safe bindings to a contract. See
/// [`ethcontract`](ethcontract) module level documentation for more information.
#[proc_macro]
pub fn contract(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let args = parse_macro_input!(input as ContractArgs);
    expand_contract(args)
        .unwrap_or_else(|e| SynError::new(Span::call_site(), e.to_string()).to_compile_error())
        .into()
}

/// Contract procedural macro arguments.
struct ContractArgs {
    artifact_path: LitStr,
    runtime_crate: Option<Ident>,
}

    // TODO(nlordell): Due to limitation with the proc-macro Span API, we can't
    //   currently get a path the the file where we were called from; therefore,
    //   the path will always be rooted on the cargo manifest directory.
    //   Eventually we can use the `Span::source_file` API to have a better
    //   experience.

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

