#![deny(missing_docs)]

//! Implementation of procedural macro for generating type-safe bindings to an
//! ethereum smart contract.

extern crate proc_macro;

use ethcontract_generate::Builder;
use proc_macro::TokenStream;
use syn::ext::IdentExt;
use syn::parse::{Error as ParseError, Parse, ParseStream, Result as ParseResult};
use syn::punctuated::Punctuated;
use syn::token::{FatArrow, Brace};
use syn::{braced, LitInt, parse_macro_input, Error as SynError, Ident, LitStr, Token};

/// Proc macro to generate type-safe bindings to a contract. See
/// [`ethcontract`](ethcontract) module level documentation for more information.
#[proc_macro]
pub fn contract(input: proc_macro::TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as ContractArgs);
    let artifact_span = args.artifact_path.span();
    args.into_builder()
        .generate()
        .map(|bindings| bindings.into_tokens())
        .unwrap_or_else(|e| SynError::new(artifact_span, e.to_string()).to_compile_error())
        .into()
}

/// Contract procedural macro arguments.
struct ContractArgs {
    artifact_path: LitStr,
    parameters: Punctuated<Parameter, Token![,]>,
}

impl ContractArgs {
    fn into_builder(self) -> Builder {
        todo!()
    }
}

impl Parse for ContractArgs {
    fn parse(input: ParseStream) -> ParseResult<Self> {
        // TODO(nlordell): Due to limitation with the proc-macro Span API, we
        //   can't currently get a path the the file where we were called from;
        //   therefore, the path will always be rooted on the cargo manifest
        //   directory. Eventually we can use the `Span::source_file` API to
        //   have a better experience.
        let artifact_path = input.parse()?;

        if !input.is_empty() {
            input.parse::<Token![,]>()?;
        }
        let parameters = input.parse_terminated(Parameter::parse)?;

        Ok(ContractArgs {
            artifact_path,
            parameters,
        })

        /*
        while !input.is_empty() {
            if input.is_empty() {
                // allow trailing commas
                break;
            }

            let param = input.call(Ident::parse_any)?;
            match param.to_string().as_str() {
                "crate" => {
                    input.parse::<Token![=]>()?;
                    let ident = input.call(Ident::parse_any)?;
                    let name = format!("{}", ident.unraw());
                    builder = builder.with_runtime_crate_name(&name);
                }
                "deployments" => {
                    braced!(content in input);
                    let deployments =
                        content.parse_terminated::<_, Token![,]>(Deployment::parse)?;
                    for deployment in deployments {}
                    let network_id: u32 = content.parse::<LitInt>()?.base10_parse()?;
                    content.parse::<Token![=>]>()?;
                    let address: LitStr = content.parse()?;
                    content.parse_ter
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
        */
    }
}

enum Parameter {
    Crate(Token![=], Ident),
    Deployments(Brace, Punctuated<Deployment, Token![,]>),
}

impl Parse for Parameter {
    fn parse(input: ParseStream) -> ParseResult<Self> {
        let name = input.call(Ident::parse_any)?;
        let param = match name.to_string().as_str() {
            "crate" => Parameter::Crate(input.parse()?, input.call(Ident::parse_any)?),
            "deployments" => {
                let content;
                Parameter::Deployments(
                    braced!(content in input),
                    content.parse_terminated(Deployment::parse)?,
                )
            }
            _ => {
                return Err(ParseError::new(
                    name.span(),
                    format!("unexpected named parameter `{}`", name),
                ))
            }
        };

        Ok(param)
    }
}

struct Deployment {
    network_id: LitInt,
    _sep: FatArrow,
    address: LitStr,
}

impl Parse for Deployment {
    fn parse(input: ParseStream) -> ParseResult<Self> {
        Ok(Deployment {
            network_id: input.parse()?,
            _sep: input.parse()?,
            address: input.parse()?,
        })
    }
}
