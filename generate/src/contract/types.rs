use crate::contract::Context;
use anyhow::{anyhow, Result};
use ethcontract_common::abi::ParamType;
use proc_macro2::{Literal, TokenStream};
use quote::quote;

pub(crate) fn expand(cx: &Context, kind: &ParamType) -> Result<TokenStream> {
    let ethcontract = &cx.runtime_crate;

    match kind {
        ParamType::Address => Ok(quote! { #ethcontract::Address }),
        ParamType::Bytes => Ok(quote! { Vec<u8> }),
        ParamType::Int(n) | ParamType::Uint(n) => match n / 8 {
            // TODO(nlordell): for now, not all uint/int types implement the
            //   `Tokenizable` trait, only `u64`, `U128`, and `U256` so we need
            //   to map solidity int/uint types to those; eventually we should
            //   add more implementations to the `web3` crate
            1..=8 => Ok(quote! { u64 }),
            9..=16 => Ok(quote! { #ethcontract::web3::types::U128 }),
            17..=32 => Ok(quote! { #ethcontract::U256 }),
            _ => Err(anyhow!("unsupported solidity type int{}", n)),
        },
        ParamType::Bool => Ok(quote! { bool }),
        ParamType::String => Ok(quote! { String }),
        ParamType::Array(t) => {
            let inner = expand(cx, t)?;
            Ok(quote! { Vec<#inner> })
        }
        ParamType::FixedBytes(n) => {
            // TODO(nlordell): what is the performance impact of returning large
            //   `FixedBytes` and `FixedArray`s with `web3`?
            let size = Literal::usize_unsuffixed(*n);
            Ok(quote! { [u8; #size] })
        }
        ParamType::FixedArray(t, n) => {
            // TODO(nlordell): see above
            let inner = expand(cx, t)?;
            let size = Literal::usize_unsuffixed(*n);
            Ok(quote! { [#inner; #size] })
        }
    }
}
