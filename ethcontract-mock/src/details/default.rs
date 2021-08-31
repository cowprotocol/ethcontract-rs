//! Helpers for building default values for tokens.

use ethcontract::common::abi::{Bytes, Int, ParamType, Token, Uint};
use ethcontract::Address;

/// Builds a default value for the given solidity type.
pub fn default(ty: &ParamType) -> Token {
    match ty {
        ParamType::Address => Token::Address(Address::default()),
        ParamType::Bytes => Token::Bytes(Bytes::default()),
        ParamType::Int(_) => Token::Int(Int::default()),
        ParamType::Uint(_) => Token::Uint(Uint::default()),
        ParamType::Bool => Token::Bool(false),
        ParamType::String => Token::String(String::default()),
        ParamType::Array(_) => Token::Array(Vec::new()),
        ParamType::FixedBytes(n) => Token::FixedBytes(vec![0; *n]),
        ParamType::FixedArray(ty, n) => Token::FixedArray(vec![default(ty); *n]),
        ParamType::Tuple(tys) => Token::Tuple(tys.iter().map(default).collect()),
    }
}
