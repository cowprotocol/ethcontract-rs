use crate::contract::{methods, types, Context};
use crate::util;
use anyhow::Result;
use ethcontract_common::abi::{Event, Hash};
use ethcontract_common::abiext::EventExt;
use inflector::Inflector;
use proc_macro2::{Literal, TokenStream};
use quote::quote;

pub(crate) fn expand(cx: &Context) -> Result<TokenStream> {
    let structs_mod = expand_structs_mod(cx)?;
    let filters = expand_filters(cx)?;
    let all_events = expand_all_events(cx);

    Ok(quote! {
        #structs_mod
        #filters
        #all_events
    })
}

fn expand_structs_mod(cx: &Context) -> Result<TokenStream> {
    let structs = cx
        .artifact
        .abi
        .events()
        .map(|event| expand_struct(event))
        .collect::<Result<Vec<_>>>()?;
    if structs.is_empty() {
        return Ok(quote! {});
    }
    Ok(quote! {
        /// Module containing all generated data models for this contract's
        /// events.
        pub mod events {
            use super::ethcontract;

            #( #structs )*
        }
    })
}

fn expand_struct(event: &Event) -> Result<TokenStream> {
    let event_name = expand_struct_name(event);

    let signature = expand_hash(event.signature());
    let abi_signature = Literal::string(&event.abi_signature());

    let params = event
        .inputs
        .iter()
        .enumerate()
        .map(|(i, input)| {
            // NOTE: Events can contain nameless values.
            let name = methods::expand_input_name(i, &input.name);
            let ty = types::expand(&input.kind)?;
            Ok((name, ty))
        })
        .collect::<Result<Vec<_>>>()?;

    let param_names = params
        .iter()
        .map(|(name, _)| name)
        .cloned()
        .collect::<Vec<_>>();
    let (params_def, params_cstr) = if event.inputs.iter().all(|input| input.name.is_empty()) {
        let fields = params
            .iter()
            .map(|(_, ty)| quote! { pub #ty, })
            .collect::<Vec<_>>();

        let defs = quote! { ( #( #fields )* ) };
        let cstr = quote! { ( #( #param_names ),* ) };

        (defs, cstr)
    } else {
        let fields = params
            .iter()
            .map(|(name, ty)| quote! { pub #name: #ty, })
            .collect::<Vec<_>>();

        let defs = quote! { { #( #fields )* } };
        let cstr = quote! { { #( #param_names ),* } };

        (defs, cstr)
    };

    let param_tokens = params
        .iter()
        .map(|(name, ty)| {
            quote! {
                let #name = #ty::from_token(tokens.next().unwrap())?;
            }
        })
        .collect::<Vec<_>>();
    let param_len = Literal::usize_unsuffixed(params.len());

    Ok(quote! {
        #[derive(Clone, Debug, Default, Eq, PartialEq)]
        pub struct #event_name #params_def

        impl #event_name {
            /// Retrieves the signature for the event this data corresponds to.
            /// This signature is the Keccak-256 hash of the ABI signature of
            /// this event.
            pub fn signature() -> self::ethcontract::H256 {
                #signature
            }

            /// Retrieves the ABI signature for the event this data corresponds
            /// to.
            pub fn abi_signature() -> &'static str {
                #abi_signature
            }
        }

        impl self::ethcontract::web3::contract::tokens::Detokenize for #event_name {
            fn from_tokens(
                tokens: Vec<self::ethcontract::private::ethabi_9_0::Token>,
            ) -> Result<Self, self::ethcontract::web3::contract::Error> {
                use self::ethcontract::web3::contract::tokens::Tokenizable;

                if tokens.len() != #param_len {
                    return Err(self::ethcontract::web3::contract::Error::InvalidOutputType(format!(
                        "Expected {} tokens, got {}: {:?}",
                        #param_len,
                        tokens.len(),
                        tokens
                    )));
                }

                #[allow(unused_mut)]
                let mut tokens = tokens.into_iter();
                #( #param_tokens )*

                Ok(#event_name #params_cstr)
            }
        }
    })
}

fn expand_struct_name(event: &Event) -> TokenStream {
    let event_name = util::ident(&event.name.to_pascal_case());
    quote! { #event_name }
}

fn expand_filters(cx: &Context) -> Result<TokenStream> {
    let filters = cx
        .artifact
        .abi
        .events()
        .filter(|event| !event.anonymous)
        .map(|event| expand_filter(event))
        .collect::<Result<Vec<_>>>()?;
    if filters.is_empty() {
        return Ok(quote! {});
    }

    Ok(quote! {
        impl Contract {
            /// Retrieves a handle to a type containing for creating event
            /// streams for all the contract events.
            pub fn events(&self) -> Events<'_> {
                Events {
                    instance: self.raw_instance(),
                }
            }
        }

        pub struct Events<'a> {
            instance: &'a self::ethcontract::DynInstance,
        }

        impl Events<'_> {
            #( #filters )*
        }
    })
}

fn expand_filter(event: &Event) -> Result<TokenStream> {
    let name = util::safe_ident(&event.name.to_snake_case());
    let data = {
        let struct_name = expand_struct_name(event);
        quote! { self::events::#struct_name }
    };
    let signature = expand_hash(event.signature());

    Ok(quote! {
        /// Generated by `ethcontract`.
        pub fn #name(&self) -> self::ethcontract::DynEventBuilder<#data> {
            self.instance.event(#signature)
                .expect("generated event filter")
        }
    })
}

fn expand_all_events(cx: &Context) -> TokenStream {
    if cx.artifact.abi.events.is_empty() {
        return quote! {};
    }

    quote! {
        impl Contract {
            /// Returns a log stream with all events.
            pub fn all_events(&self) -> self::ethcontract::DynLogStream {
                self.raw_instance().all_events()
            }
        }
    }
}

/// Expands a 256-bit `Hash` into a literal representation that can be used with
/// quasi-quoting for code generation.
fn expand_hash(hash: Hash) -> TokenStream {
    let bytes = hash.as_bytes().iter().copied().map(Literal::u8_unsuffixed);

    quote! {
        self::ethcontract::H256::from([#( #bytes ),*])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethcontract_common::abi::{EventParam, ParamType};

    #[test]
    fn expand_empty_filters() {
        assert_quote!(expand_filters(&Context::default()).unwrap(), {});
    }

    #[test]
    fn expand_transfer_filter() {
        let event = Event {
            name: "Transfer".into(),
            inputs: vec![
                EventParam {
                    name: "from".into(),
                    kind: ParamType::Address,
                    indexed: true,
                },
                EventParam {
                    name: "to".into(),
                    kind: ParamType::Address,
                    indexed: true,
                },
                EventParam {
                    name: "amount".into(),
                    kind: ParamType::Uint(256),
                    indexed: false,
                },
            ],
            anonymous: false,
        };
        let signature = expand_hash(event.signature());

        assert_quote!(expand_filter(&event).unwrap(), {
            /// Generated by `ethcontract`.
            pub fn transfer(&self) -> self::ethcontract::DynEventBuilder<(
                self::ethcontract::Address,
                self::ethcontract::Address,
                self::ethcontract::U256
            )> {
                self.instance.event(#signature).expect("generated event filter")
            }
        });
    }

    #[test]
    #[rustfmt::skip]
    fn expand_hash_value() {
        assert_quote!(
            expand_hash(
                "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f".parse().unwrap()
            ),
            {
                self::ethcontract::H256::from([
                    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
                    16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31
                ])
            },
        );
    }
}
