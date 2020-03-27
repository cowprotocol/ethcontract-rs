use crate::contract::{types, Context};
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

/// Expands into a module containing all the event data structures from the ABI.
fn expand_structs_mod(cx: &Context) -> Result<TokenStream> {
    let data_types = cx
        .artifact
        .abi
        .events()
        .map(|event| expand_data_type(event))
        .collect::<Result<Vec<_>>>()?;
    if data_types.is_empty() {
        return Ok(quote! {});
    }

    Ok(quote! {
        /// Module containing all generated data models for this contract's
        /// events.
        pub mod events {
            use super::ethcontract;

            #( #data_types )*
        }
    })
}

/// Expands an ABI event into a single event data type. This can expand either
/// into a structure or a tuple in the case where all event parameters (topics
/// and data) are anonymous.
fn expand_data_type(event: &Event) -> Result<TokenStream> {
    let event_name = expand_struct_name(event);

    let signature = expand_hash(event.signature());

    let abi_signature = event.abi_signature();
    let abi_signature_lit = Literal::string(&abi_signature);
    let abi_signature_doc = util::expand_doc(&format!("`{}`", abi_signature));

    let params = expand_params(event)?;

    let all_anonymous_fields = event.inputs.iter().all(|input| input.name.is_empty());
    let (data_type_definition, data_type_construction) = if all_anonymous_fields {
        expand_data_tuple(&event_name, &params)
    } else {
        expand_data_struct(&event_name, &params)
    };

    let params_len = Literal::usize_unsuffixed(params.len());
    let read_param_token = params
        .iter()
        .map(|(name, ty)| {
            quote! {
                let #name = #ty::from_token(tokens.next().unwrap())?;
            }
        })
        .collect::<Vec<_>>();

    Ok(quote! {
        #[derive(Clone, Debug, Default, Eq, PartialEq)]
        pub #data_type_definition

        impl #event_name {
            /// Retrieves the signature for the event this data corresponds to.
            /// This signature is the Keccak-256 hash of the ABI signature of
            /// this event.
            pub fn signature() -> self::ethcontract::H256 {
                #signature
            }

            /// Retrieves the ABI signature for the event this data corresponds
            /// to. For this event the value should always be:
            ///
            #abi_signature_doc
            pub fn abi_signature() -> &'static str {
                #abi_signature_lit
            }
        }

        impl self::ethcontract::web3::contract::tokens::Detokenize for #event_name {
            fn from_tokens(
                tokens: Vec<self::ethcontract::private::ethabi_9_0::Token>,
            ) -> Result<Self, self::ethcontract::web3::contract::Error> {
                use self::ethcontract::web3::contract::tokens::Tokenizable;

                if tokens.len() != #params_len {
                    return Err(self::ethcontract::web3::contract::Error::InvalidOutputType(format!(
                        "Expected {} tokens, got {}: {:?}",
                        #params_len,
                        tokens.len(),
                        tokens
                    )));
                }

                #[allow(unused_mut)]
                let mut tokens = tokens.into_iter();
                #( #read_param_token )*

                Ok(#data_type_construction)
            }
        }
    })
}

/// Expands an ABI event into an identifier for its event data type.
fn expand_struct_name(event: &Event) -> TokenStream {
    let event_name = util::ident(&event.name.to_pascal_case());
    quote! { #event_name }
}

/// Expands an ABI event into name-type pairs for each of its parameters.
fn expand_params(event: &Event) -> Result<Vec<(TokenStream, TokenStream)>> {
    event
        .inputs
        .iter()
        .enumerate()
        .map(|(i, input)| {
            // NOTE: Events can contain nameless values.
            let name = util::expand_input_name(i, &input.name);
            let ty = types::expand(&input.kind)?;
            Ok((name, ty))
        })
        .collect()
}

/// Expands an event data structure from its name-type parameter pairs. Returns
/// a tuple with the type definition (i.e. the struct declaration) and
/// construction (i.e. code for creating an instance of the event data).
fn expand_data_struct(
    name: &TokenStream,
    params: &[(TokenStream, TokenStream)],
) -> (TokenStream, TokenStream) {
    let fields = params
        .iter()
        .map(|(name, ty)| quote! { pub #name: #ty })
        .collect::<Vec<_>>();

    let param_names = params
        .iter()
        .map(|(name, _)| name)
        .cloned()
        .collect::<Vec<_>>();

    let definition = quote! { struct #name { #( #fields, )* } };
    let construction = quote! { #name { #( #param_names ),* } };

    (definition, construction)
}

/// Expands an event data named tuple from its name-type parameter pairs.
/// Returns a tuple with the type definition and construction.
fn expand_data_tuple(
    name: &TokenStream,
    params: &[(TokenStream, TokenStream)],
) -> (TokenStream, TokenStream) {
    let fields = params
        .iter()
        .map(|(_, ty)| quote! { pub #ty })
        .collect::<Vec<_>>();

    let param_names = params
        .iter()
        .map(|(name, _)| name)
        .cloned()
        .collect::<Vec<_>>();

    let definition = quote! { struct #name( #( #fields ),* ); };
    let construction = quote! { #name( #( #param_names ),* ) };

    (definition, construction)
}

/// Expands into an `Events` type with method definitions for creating event
/// streams for all non-anonymous contract events in the ABI.
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
            instance: &'a self::ethcontract::dyns::DynInstance,
        }

        impl Events<'_> {
            #( #filters )*
        }
    })
}

/// Expands into a single method for contracting an event stream.
fn expand_filter(event: &Event) -> Result<TokenStream> {
    let name = util::safe_ident(&event.name.to_snake_case());
    let data = {
        let struct_name = expand_struct_name(event);
        quote! { self::events::#struct_name }
    };
    let signature = expand_hash(event.signature());

    Ok(quote! {
        /// Generated by `ethcontract`.
        pub fn #name(&self) -> self::ethcontract::dyns::DynEventBuilder<#data> {
            self.instance.event(#signature)
                .expect("generated event filter")
        }
    })
}

/// Expands into the `all_events` method on the root contract type if it
/// contains events. Expands to nothing otherwise.
fn expand_all_events(cx: &Context) -> TokenStream {
    if cx.artifact.abi.events.is_empty() {
        return quote! {};
    }

    quote! {
        impl Contract {
            /// Returns a log stream with all events.
            pub fn all_events(&self) -> self::ethcontract::dyns::DynAllEventsBuilder<
                self::ethcontract::RawEventData,
            > {
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
            pub fn transfer(&self) -> self::ethcontract::dyns::DynEventBuilder<self::events::Transfer> {
                self.instance.event(#signature).expect("generated event filter")
            }
        });
    }

    #[test]
    fn expand_data_struct_value() {
        let event = Event {
            name: "Foo".into(),
            inputs: vec![
                EventParam {
                    name: "a".into(),
                    kind: ParamType::Bool,
                    indexed: false,
                },
                EventParam {
                    name: String::new(),
                    kind: ParamType::Address,
                    indexed: false,
                },
            ],
            anonymous: false,
        };

        let name = expand_struct_name(&event);
        let params = expand_params(&event).unwrap();
        let (definition, construction) = expand_data_struct(&name, &params);

        assert_quote!(definition, {
            struct Foo {
                pub a: bool,
                pub p1: self::ethcontract::Address,
            }
        });
        assert_quote!(construction, { Foo { a, p1 } });
    }

    #[test]
    fn expand_data_tuple_value() {
        let event = Event {
            name: "Foo".into(),
            inputs: vec![
                EventParam {
                    name: String::new(),
                    kind: ParamType::Bool,
                    indexed: false,
                },
                EventParam {
                    name: String::new(),
                    kind: ParamType::Address,
                    indexed: false,
                },
            ],
            anonymous: false,
        };

        let name = expand_struct_name(&event);
        let params = expand_params(&event).unwrap();
        let (definition, construction) = expand_data_tuple(&name, &params);

        assert_quote!(definition, {
            struct Foo(pub bool, pub self::ethcontract::Address);
        });
        assert_quote!(construction, { Foo(p0, p1) });
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
