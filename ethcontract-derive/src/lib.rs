#![deny(missing_docs, unsafe_code)]

//! Implementation of procedural macro for generating type-safe bindings to an
//! ethereum smart contract.

extern crate proc_macro;

mod spanned;

use crate::spanned::{ParseInner, Spanned};
use anyhow::{anyhow, Result};
use ethcontract_common::abi::{Function, Param, ParamType};
use ethcontract_common::abiext::{FunctionExt, ParamTypeExt};
use ethcontract_common::artifact::truffle::TruffleLoader;
use ethcontract_common::contract::Network;
use ethcontract_common::Address;
use ethcontract_generate::loaders::{HardHatFormat, HardHatLoader};
use ethcontract_generate::{parse_address, ContractBuilder, Source};
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{quote, ToTokens as _};
use std::collections::HashSet;
use syn::ext::IdentExt;
use syn::parse::{Error as ParseError, Parse, ParseStream, Result as ParseResult};
use syn::{
    braced, parenthesized, parse_macro_input, Error as SynError, Ident, LitInt, LitStr, Path,
    Token, Visibility,
};

/// Proc macro to generate type-safe bindings to a contract.
///
/// This macro accepts a path to an artifact JSON file. Note that this path
/// is rooted in the crate's root `CARGO_MANIFEST_DIR`:
///
/// ```ignore
/// contract!("build/contracts/WETH9.json");
/// ```
///
/// Alternatively, other sources may be used, for full details consult the
/// [`ethcontract_generate::source`] documentation. Some basic examples:
///
/// ```ignore
/// // HTTP(S) source
/// contract!("https://my.domain.local/path/to/contract.json")
///
/// // etherscan.io
/// contract!("etherscan:0xC02AAA39B223FE8D0A0E5C4F27EAD9083C756CC2");
///
/// // npm package
/// contract!("npm:@openzeppelin/contracts@4.2.0/build/contracts/IERC20.json")
/// ```
///
/// Note that etherscan rate-limits requests to their API, to avoid this an
/// `ETHERSCAN_API_KEY` environment variable can be set. If it is, it will use
/// that API key when retrieving the contract ABI.
///
/// Currently, the proc macro accepts additional parameters to configure some
/// aspects of the code generation. Specifically it accepts the following.
///
/// - `format`: format of the artifact.
///
///   Available values are:
///
///   - `truffle` (default) to use [truffle loader];
///   - `hardhat` to use [hardhat loader] in [single export mode];
///   - `hardhat_multi` to use hardhat loader in [multi export mode].
///
///   Note that hardhat artifacts export multiple contracts. You'll have to use
///   `contract` parameter to specify which contract to generate bindings to.
///
///   [truffle loader]: ethcontract_common::artifact::truffle::TruffleLoader
///   [hardhat loader]: ethcontract_common::artifact::hardhat::HardHatLoader
///   [single export mode]: ethcontract_common::artifact::hardhat::Format::SingleExport
///   [multi export mode]: ethcontract_common::artifact::hardhat::Format::MultiExport
///
/// - `contract`: name of the contract we're generating bindings to.
///
///   If an artifact exports a single unnamed artifact, this parameter
///   can be used to set its name. For example:
///
///   ```ignore
///   contract!(
///       "etherscan:0xC02AAA39B223FE8D0A0E5C4F27EAD9083C756CC2",
///       contract = WETH9
///   );
///   ```
///
///   Otherwise, it can be used to specify which contract we're generating
///   bindings to. Additionally, you can rename contract class by specifying
///   a new name after the `as` keyword. For example:
///
///   ```ignore
///   contract!(
///       "build/contracts.json",
///       format = hardhat_multi,
///       contract = WETH9 as WrappedEthereum
///   );
///   ```
///
/// - `mod`: name of the contract module to place generated code in.
///
///   This defaults to the contract name converted into snake case.
///
///   Note that the root contract type gets re-exported in the context where the
///   macro was invoked.
///
///   Example:
///
///   ```ignore
///   contract!(
///       "build/contracts/WETH9.json",
///       contract = WETH9 as WrappedEthereum,
///       mod = weth,
///   );
///   ```
///
/// - `deployments`: a list of additional addresses of deployed contract for
///   specified network IDs.
///
///   This mapping allows generated contract's `deployed` function to work
///   with networks that are not included in the artifact's deployment
///   information.
///
///   Note that deployments defined this way **take precedence** over
///   the ones defined in the artifact.
///
///   This parameter is intended to be used to manually specify contract
///   addresses for test environments, be it testnet addresses that may defer
///   from the originally published artifact or deterministic contract
///   addresses on local development nodes.
///
///   Example:
///
///   ```ignore
///   contract!(
///       "build/contracts/WETH9.json",
///       deployments {
///           4 => "0x000102030405060708090a0b0c0d0e0f10111213",
///           5777 => "0x0123456789012345678901234567890123456789",
///       },
///   );
///   ```
///
/// - `methods`: a list of mappings from method signatures to method names
///   allowing methods names to be explicitly set for contract methods.
///
///   This also provides a workaround for generating code for contracts
///   with multiple methods with the same name.
///
///   Example:
///
///   ```ignore
///   contract!(
///       "build/contracts/WETH9.json",
///       methods {
///           approve(Address, U256) as set_allowance
///       },
///   );
///   ```
///
/// - `event_derives`: a list of additional derives that should be added to
///   contract event structs and enums.
///
///   Example:
///
///   ```ignore
///   contract!(
///       "build/contracts/WETH9.json",
///       event_derives (serde::Deserialize, serde::Serialize),
///   );
///   ```
///
/// - `crate`: the name of the `ethcontract` crate. This is useful if the crate
///   was renamed in the `Cargo.toml` for whatever reason.
///
/// Additionally, the ABI source can be preceded by a visibility modifier such
/// as `pub` or `pub(crate)`. This visibility modifier is applied to both the
/// generated module and contract re-export. If no visibility modifier is
/// provided, then none is used for the generated code as well, making the
/// module and contract private to the scope where the macro was invoked.
///
/// Full example:
///
/// ```ignore
/// contract!(
///     pub(crate) "build/contracts.json",
///     format = hardhat_multi,
///     contract = WETH9 as WrappedEthereum,
///     mod = weth,
///     deployments {
///         4 => "0x000102030405060708090a0b0c0d0e0f10111213",
///         5777 => "0x0123456789012345678901234567890123456789",
///     },
///     methods {
///         myMethod(uint256,bool) as my_renamed_method;
///     },
///     event_derives (serde::Deserialize, serde::Serialize),
///     crate = ethcontract_renamed,
/// );
/// ```
///
/// See [`ethcontract`](ethcontract) module level documentation for additional
/// information.
#[proc_macro]
pub fn contract(input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as Spanned<ContractArgs>);
    let span = args.span();
    generate(args.into_inner())
        .unwrap_or_else(|e| SynError::new(span, format!("{:?}", e)).to_compile_error())
        .into()
}

fn generate(args: ContractArgs) -> Result<TokenStream2> {
    let mut artifact_format = Format::Truffle;
    let mut contract_name = None;

    let mut builder = ContractBuilder::new();
    builder.visibility_modifier = args.visibility;

    for parameter in args.parameters.into_iter() {
        match parameter {
            Parameter::Mod(name) => builder.contract_mod_override = Some(name),
            Parameter::Contract(name, alias) => {
                builder.contract_name_override = alias.or_else(|| Some(name.clone()));
                contract_name = Some(name);
            }
            Parameter::Crate(name) => builder.runtime_crate_name = name,
            Parameter::Deployments(deployments) => {
                for deployment in deployments {
                    builder.networks.insert(
                        deployment.network_id.to_string(),
                        Network {
                            address: deployment.address,
                            deployment_information: None,
                        },
                    );
                }
            }
            Parameter::Methods(methods) => {
                for method in methods {
                    builder
                        .method_aliases
                        .insert(method.signature, method.alias);
                }
            }
            Parameter::EventDerives(derives) => {
                builder.event_derives.extend(derives);
            }
            Parameter::Format(format) => artifact_format = format,
        };
    }

    let source = Source::parse(&args.artifact_path)?;
    let json = source.artifact_json()?;

    match artifact_format {
        Format::Truffle => {
            let mut contract = TruffleLoader::new().load_contract_from_str(&json)?;

            if let Some(contract_name) = contract_name {
                if contract.name.is_empty() {
                    contract.name = contract_name;
                } else if contract.name != contract_name {
                    return Err(anyhow!(
                        "there is no contract '{}' in artifact '{}'",
                        contract_name,
                        args.artifact_path
                    ));
                }
            }

            Ok(builder.generate(&contract)?.into_tokens())
        }

        Format::HardHat(format) => {
            let artifact = HardHatLoader::new(format).load_from_str(&json)?;

            if let Some(contract_name) = contract_name {
                if let Some(contract) = artifact.get(&contract_name) {
                    Ok(builder.generate(contract)?.into_tokens())
                } else {
                    Err(anyhow!(
                        "there is no contract '{}' in artifact '{}'",
                        contract_name,
                        args.artifact_path
                    ))
                }
            } else {
                Err(anyhow!(
                    "when using hardhat artifacts, you should specify \
                     contract name using 'contract' parameter"
                ))
            }
        }
    }
}

/// Contract procedural macro arguments.
#[cfg_attr(test, derive(Debug, Eq, PartialEq))]
struct ContractArgs {
    visibility: Option<String>,
    artifact_path: String,
    parameters: Vec<Parameter>,
}

impl ParseInner for ContractArgs {
    fn spanned_parse(input: ParseStream) -> ParseResult<(Span, Self)> {
        let visibility = match input.parse::<Visibility>()? {
            Visibility::Inherited => None,
            token => Some(quote!(#token).to_string()),
        };

        // TODO(nlordell): Due to limitation with the proc-macro Span API, we
        //   can't currently get a path the the file where we were called from;
        //   therefore, the path will always be rooted on the cargo manifest
        //   directory. Eventually we can use the `Span::source_file` API to
        //   have a better experience.
        let (span, artifact_path) = {
            let literal = input.parse::<LitStr>()?;
            (literal.span(), literal.value())
        };

        if !input.is_empty() {
            input.parse::<Token![,]>()?;
        }
        let parameters = input
            .parse_terminated::<_, Token![,]>(Parameter::parse)?
            .into_iter()
            .collect();

        Ok((
            span,
            ContractArgs {
                visibility,
                artifact_path,
                parameters,
            },
        ))
    }
}

/// Artifact format
#[cfg_attr(test, derive(Debug, Eq, PartialEq))]
enum Format {
    Truffle,
    HardHat(HardHatFormat),
}

/// A single procedural macro parameter.
#[cfg_attr(test, derive(Debug, Eq, PartialEq))]
enum Parameter {
    Mod(String),
    Contract(String, Option<String>),
    Crate(String),
    Deployments(Vec<Deployment>),
    Methods(Vec<Method>),
    EventDerives(Vec<String>),
    Format(Format),
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
            "mod" => {
                input.parse::<Token![=]>()?;
                let name = input.parse::<Ident>()?.to_string();
                Parameter::Mod(name)
            }
            "format" => {
                input.parse::<Token![=]>()?;
                let token = input.parse::<Ident>()?;
                let format = match token.to_string().as_str() {
                    "truffle" => Format::Truffle,
                    "hardhat" => Format::HardHat(HardHatFormat::SingleExport),
                    "hardhat_multi" => Format::HardHat(HardHatFormat::MultiExport),
                    format => {
                        return Err(ParseError::new(
                            token.span(),
                            format!("unknown format {}", format),
                        ))
                    }
                };
                Parameter::Format(format)
            }
            "contract" => {
                input.parse::<Token![=]>()?;
                let name = input.parse::<Ident>()?.to_string();
                let alias = if input.parse::<Option<Token![as]>>()?.is_some() {
                    Some(input.parse::<Ident>()?.to_string())
                } else {
                    None
                };

                Parameter::Contract(name, alias)
            }
            "deployments" => {
                let content;
                braced!(content in input);
                let deployments = {
                    let parsed =
                        content.parse_terminated::<_, Token![,]>(Spanned::<Deployment>::parse)?;

                    let mut deployments = Vec::with_capacity(parsed.len());
                    let mut networks = HashSet::new();
                    for deployment in parsed {
                        if !networks.insert(deployment.network_id) {
                            return Err(ParseError::new(
                                deployment.span(),
                                "duplicate network ID in `ethcontract::contract!` macro invocation",
                            ));
                        }
                        deployments.push(deployment.into_inner())
                    }

                    deployments
                };

                Parameter::Deployments(deployments)
            }
            "methods" => {
                let content;
                braced!(content in input);
                let methods = {
                    let parsed =
                        content.parse_terminated::<_, Token![;]>(Spanned::<Method>::parse)?;

                    let mut methods = Vec::with_capacity(parsed.len());
                    let mut signatures = HashSet::new();
                    let mut aliases = HashSet::new();
                    for method in parsed {
                        if !signatures.insert(method.signature.clone()) {
                            return Err(ParseError::new(
                                method.span(),
                                "duplicate method signature in `ethcontract::contract!` macro invocation",
                            ));
                        }
                        if !aliases.insert(method.alias.clone()) {
                            return Err(ParseError::new(
                                method.span(),
                                "duplicate method alias in `ethcontract::contract!` macro invocation",
                            ));
                        }
                        methods.push(method.into_inner())
                    }

                    methods
                };

                Parameter::Methods(methods)
            }
            "event_derives" => {
                let content;
                parenthesized!(content in input);
                let derives = content
                    .parse_terminated::<_, Token![,]>(Path::parse)?
                    .into_iter()
                    .map(|path| path.to_token_stream().to_string())
                    .collect();
                Parameter::EventDerives(derives)
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

/// A manually specified dependency.
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

/// An explicitely named contract method.
#[cfg_attr(test, derive(Debug, Eq, PartialEq))]
struct Method {
    signature: String,
    alias: String,
}

impl Parse for Method {
    fn parse(input: ParseStream) -> ParseResult<Self> {
        let function = {
            let name = input.parse::<Ident>()?.to_string();

            let content;
            parenthesized!(content in input);
            let inputs = content
                .parse_terminated::<_, Token![,]>(Ident::parse)?
                .iter()
                .map(|ident| {
                    let kind = ParamType::from_str(&ident.to_string())
                        .map_err(|err| ParseError::new(ident.span(), err))?;
                    Ok(Param {
                        name: "".into(),
                        kind,
                    })
                })
                .collect::<ParseResult<Vec<_>>>()?;

            #[allow(deprecated)]
            Function {
                name,
                inputs,

                // NOTE: The output types and const-ness of the function do not
                //   affect its signature.
                outputs: vec![],
                constant: false,
                state_mutability: Default::default(),
            }
        };
        let signature = function.abi_signature();
        input.parse::<Token![as]>()?;
        let alias = {
            let ident = input.parse::<Ident>()?;
            ident.to_string()
        };

        Ok(Method { signature, alias })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! contract_args_result {
        ($($arg:tt)*) => {{
            use syn::parse::Parser;
            <Spanned<ContractArgs> as Parse>::parse
                .parse2(quote::quote! { $($arg)* })
        }};
    }
    macro_rules! contract_args {
        ($($arg:tt)*) => {
            contract_args_result!($($arg)*)
                .expect("failed to parse contract args")
                .into_inner()
        };
    }
    macro_rules! contract_args_err {
        ($($arg:tt)*) => {
            contract_args_result!($($arg)*)
                .expect_err("expected parse contract args to error")
        };
    }

    fn deployment(network_id: u32, address: &str) -> Deployment {
        Deployment {
            network_id,
            address: parse_address(address).expect("failed to parse deployment address"),
        }
    }

    fn method(signature: &str, alias: &str) -> Method {
        Method {
            signature: signature.into(),
            alias: alias.into(),
        }
    }

    #[test]
    fn parse_contract_args() {
        let args = contract_args!("path/to/artifact.json");
        assert_eq!(args.artifact_path, "path/to/artifact.json");
    }

    #[test]
    fn crate_parameter_accepts_keywords() {
        let args = contract_args!("artifact.json", crate = crate);
        assert_eq!(args.parameters, &[Parameter::Crate("crate".into())]);
    }

    #[test]
    fn parse_contract_args_with_defaults() {
        let args = contract_args!("artifact.json");
        assert_eq!(
            args,
            ContractArgs {
                visibility: None,
                artifact_path: "artifact.json".into(),
                parameters: vec![],
            },
        );
    }

    #[test]
    fn parse_contract_args_with_parameters() {
        let args = contract_args!(
            pub(crate) "artifact.json",
            crate = foobar,
            mod = contract,
            contract = Contract,
            deployments {
                1 => "0x000102030405060708090a0b0c0d0e0f10111213",
                4 => "0x0123456789012345678901234567890123456789",
            },
            methods {
                myMethod(uint256, bool) as my_renamed_method;
                myOtherMethod() as my_other_renamed_method;
            },
            event_derives (Asdf, a::B, a::b::c::D)
        );
        assert_eq!(
            args,
            ContractArgs {
                visibility: Some(quote!(pub(crate)).to_string()),
                artifact_path: "artifact.json".into(),
                parameters: vec![
                    Parameter::Crate("foobar".into()),
                    Parameter::Mod("contract".into()),
                    Parameter::Contract("Contract".into(), None),
                    Parameter::Deployments(vec![
                        deployment(1, "0x000102030405060708090a0b0c0d0e0f10111213"),
                        deployment(4, "0x0123456789012345678901234567890123456789"),
                    ]),
                    Parameter::Methods(vec![
                        method("myMethod(uint256,bool)", "my_renamed_method"),
                        method("myOtherMethod()", "my_other_renamed_method"),
                    ]),
                    Parameter::EventDerives(vec![
                        "Asdf".into(),
                        "a :: B".into(),
                        "a :: b :: c :: D".into()
                    ])
                ],
            },
        );
    }

    #[test]
    fn parse_contract_args_format() {
        let args = contract_args!("artifact.json", format = hardhat_multi);
        assert_eq!(
            args,
            ContractArgs {
                visibility: None,
                artifact_path: "artifact.json".into(),
                parameters: vec![Parameter::Format(Format::HardHat(
                    HardHatFormat::MultiExport
                ))],
            },
        );
    }

    #[test]
    fn parse_contract_args_rename() {
        let args = contract_args!("artifact.json", contract = Contract as Renamed);
        assert_eq!(
            args,
            ContractArgs {
                visibility: None,
                artifact_path: "artifact.json".into(),
                parameters: vec![Parameter::Contract("Contract".into(), Some("Renamed".into()))],
            },
        );
    }

    #[test]
    fn unsupported_format_error() {
        contract_args_err!("artifact.json", format = yaml,);
    }

    #[test]
    fn duplicate_network_id_error() {
        contract_args_err!(
            "artifact.json",
            deployments {
                1 => "0x000102030405060708090a0b0c0d0e0f10111213",
                1 => "0x0123456789012345678901234567890123456789",
            }
        );
    }

    #[test]
    fn duplicate_method_rename_error() {
        contract_args_err!(
            "artifact.json",
            methods {
                myMethod(uint256) as my_method_1;
                myMethod(uint256) as my_method_2;
            }
        );
        contract_args_err!(
            "artifact.json",
            methods {
                myMethod1(uint256) as my_method;
                myMethod2(uint256) as my_method;
            }
        );
    }

    #[test]
    fn method_invalid_method_parameter_type() {
        contract_args_err!(
            "artifact.json",
            methods {
                myMethod(invalid) as my_method;
            }
        );
    }
}
