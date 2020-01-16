#![deny(missing_docs)]

//! Implementation of procedural macro for generating type-safe bindings to an
//! ethereum smart contract.

extern crate proc_macro;

use ethcontract_generate::{parse_address, Address, Builder};
use proc_macro::TokenStream;
use syn::ext::IdentExt;
use syn::parse::{Error as ParseError, Parse, ParseStream, Result as ParseResult};
use syn::{braced, parse_macro_input, Error as SynError, Ident, LitInt, LitStr, Token};

/// Proc macro to generate type-safe bindings to a contract. See
/// [`ethcontract`](ethcontract) module level documentation for more information.
#[proc_macro]
pub fn contract(input: TokenStream) -> TokenStream {
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
    parameters: Vec<Parameter>,
}

impl ContractArgs {
    fn into_builder(self) -> Builder {
        let mut builder = Builder::new(&self.artifact_path.value());
        for parameter in self.parameters.into_iter() {
            builder = match parameter {
                Parameter::Crate(name) => builder.with_runtime_crate_name(name),
                Parameter::Deployments(deployments) => {
                    deployments.into_iter().fold(builder, |builder, d| {
                        builder.add_deployment(d.network_id, d.address)
                    })
                }
            };
        }
        builder
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
        let parameters = input
            .parse_terminated::<_, Token![,]>(Parameter::parse)?
            .into_iter()
            .collect();

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

#[cfg_attr(test, derive(Debug, Eq, PartialEq))]
enum Parameter {
    Crate(String),
    Deployments(Vec<Deployment>),
}

impl Parse for Parameter {
    fn parse(input: ParseStream) -> ParseResult<Self> {
        let name = input.call(Ident::parse_any)?;
        let param = match name.to_string().as_str() {
            "crate" => {
                input.parse::<Token![=]>()?;
                let name = input.call(Ident::parse_any)?.to_string();

                Parameter::Crate(name)
            }
            "deployments" => {
                let content;
                braced!(content in input);
                let deployments = content
                    .parse_terminated::<_, Token![,]>(Deployment::parse)?
                    .into_iter()
                    .collect();

                Parameter::Deployments(deployments)
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

#[cfg_attr(test, derive(Debug, Eq, PartialEq))]
struct Deployment {
    network_id: u32,
    address: Address,
}

impl Parse for Deployment {
    fn parse(input: ParseStream) -> ParseResult<Self> {
        let network_id = input.parse::<LitInt>()?.base10_parse()?;
        input.parse::<Token![=>]>()?;
        let address = {
            let literal = input.parse::<LitStr>()?;
            parse_address(&literal.value()).map_err(|err| ParseError::new(literal.span(), err))?
        };

        Ok(Deployment {
            network_id,
            address,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! contract_args {
        ($($arg:tt)*) => {{
            use syn::parse::Parser;
            <ContractArgs as Parse>::parse
                .parse2(quote::quote! { $($arg)* })
                .expect("failed to parse contract args")
        }};
    }

    fn deployment(network_id: u32, address: &str) -> Deployment {
        Deployment {
            network_id,
            address: parse_address(address).expect("failed to parse deployment address"),
        }
    }

    #[test]
    fn parse_contract_args() {
        let args = contract_args!("path/to/artifact.json");
        assert_eq!(args.artifact_path.value(), "path/to/artifact.json");
    }

    #[test]
    fn parse_contract_args_with_parameter() {
        let args = contract_args!("artifact.json", crate = foobar);
        assert_eq!(args.parameters, &[Parameter::Crate("foobar".into())]);
    }

    #[test]
    fn crate_parameter_accepts_keywords() {
        let args = contract_args!("artifact.json", crate = crate);
        assert_eq!(args.parameters, &[Parameter::Crate("crate".into())]);
    }

    #[test]
    fn parse_contract_args_with_parameters() {
        let args = contract_args!(
            "artifact.json",
            crate = foobar,
            deployments {
                1 => "0x000102030405060708090a0b0c0d0e0f10111213",
                4 => "0x0123456789012345678901234567890123456789",
            },
        );
        assert_eq!(
            args.parameters,
            &[
                Parameter::Crate("foobar".into()),
                Parameter::Deployments(vec![
                    deployment(1, "0x000102030405060708090a0b0c0d0e0f10111213"),
                    deployment(4, "0x0123456789012345678901234567890123456789"),
                ]),
            ]
        );
    }
}
