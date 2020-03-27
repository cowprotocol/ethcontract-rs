use crate::contract::{types, Context};
use crate::util;
use anyhow::Result;
use ethcontract_common::abi::{Event, Hash};
use inflector::Inflector;
use proc_macro2::{Literal, TokenStream};
use quote::quote;

pub(crate) fn expand(cx: &Context) -> Result<TokenStream> {
    let filters = expand_filters(cx)?;
    let all_events = expand_all_events(cx);

    Ok(quote! {
        #filters
        #all_events
    })
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
            instance: &'a self::ethcontract::dyns::DynInstance,
        }

        impl Events<'_> {
            #( #filters )*
        }
    })
}

fn expand_filter(event: &Event) -> Result<TokenStream> {
    let name = util::safe_ident(&event.name.to_snake_case());
    let data = {
        let inputs = event
            .inputs
            .iter()
            .map(|input| types::expand(&input.kind))
            .collect::<Result<Vec<_>>>()?;
        quote! { ( #( #inputs ),* ) }
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

fn expand_all_events(cx: &Context) -> TokenStream {
    if cx.artifact.abi.events.is_empty() {
        return quote! {};
    }

    quote! {
        impl Contract {
            /// Returns a log stream with all events.
            pub fn all_events(&self) -> self::ethcontract::dyns::DynLogStream {
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
    fn expand_hash_value() {
        #[rustfmt::skip]
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
