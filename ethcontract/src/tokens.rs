//! Tokenization related functionality allowing rust types to be mapped to solidity types.

// This file is based on https://github.com/tomusdrw/rust-web3/blob/e6d044a28458be9a3ee31108475d787e0440ce8b/src/contract/tokens.rs .
// Generated contract bindings should operate on native rust types for ease of use. To encode them
// with ethabi we need to map them to ethabi tokens. Tokenize does this for base types like
// u32 and compounds of other Tokenize in the form of vectors, arrays and tuples.
//
// In some cases like when passing arguments to `MethodBuilder` or decoding events we need to be
// able to pack multiple types into a single generic parameter. This is accomplished by representing
// the collection of arguments as a tuple.
//
// A completely different approach could be to avoid using the trait system and instead encode all
// rust types into tokens directly in the ethcontract generated bindings.

use crate::I256;
use arrayvec::ArrayVec;
use ethcontract_common::{abi::Token, TransactionHash};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use web3::types::{Address, U256};

/// A tokenization related error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Tokenize::from_token token type doesn't match the rust type.
    #[error("expected a different token type")]
    TypeMismatch,
    /// Tokenize::from_token is called with integer that doesn't fit in the rust type.
    #[error("abi integer is does not fit rust integer")]
    IntegerMismatch,
    /// Tokenize::from_token token is fixed bytes with wrong length.
    #[error("expected a different number of fixed bytes")]
    FixedBytesLengthsMismatch,
    /// Tokenize::from_token token is fixed array with wrong length.
    #[error("expected a different number of tokens in fixed array")]
    FixedArrayLengthsMismatch,
    /// Tokenize::from_token token is tuple with wrong length.
    #[error("expected a different number of tokens in tuple")]
    TupleLengthMismatch,
}

/// Rust type and single token conversion.
pub trait Tokenize {
    /// Convert self into token.
    fn from_token(token: Token) -> Result<Self, Error>
    where
        Self: Sized;

    /// Convert token into Self.
    fn into_token(self) -> Token;
}

/// Wrapper around Vec<u8> and [u8; N] representing Token::{Bytes, FixedBytes}. Distinguishes a list
/// of u8 from bytes.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, PartialOrd, Ord, Serialize,
)]
pub struct Bytes<T>(pub T);

impl Tokenize for Bytes<Vec<u8>> {
    fn from_token(token: Token) -> Result<Self, Error>
    where
        Self: Sized,
    {
        match token {
            Token::Bytes(bytes) => Ok(Self(bytes)),
            _ => Err(Error::TypeMismatch),
        }
    }

    fn into_token(self) -> Token {
        Token::Bytes(self.0)
    }
}

impl<const N: usize> Tokenize for Bytes<[u8; N]> {
    fn from_token(token: Token) -> Result<Self, Error> {
        match token {
            Token::FixedBytes(bytes) => bytes
                .try_into()
                .map(Self)
                .map_err(|_| Error::FixedBytesLengthsMismatch),
            _ => Err(Error::TypeMismatch),
        }
    }

    fn into_token(self) -> Token {
        Token::FixedBytes(self.0.to_vec())
    }
}

impl Tokenize for String {
    fn from_token(token: Token) -> Result<Self, Error> {
        match token {
            Token::String(s) => Ok(s),
            _ => Err(Error::TypeMismatch),
        }
    }

    fn into_token(self) -> Token {
        Token::String(self)
    }
}

impl Tokenize for Address {
    fn from_token(token: Token) -> Result<Self, Error> {
        match token {
            Token::Address(data) => Ok(data),
            _ => Err(Error::TypeMismatch),
        }
    }

    fn into_token(self) -> Token {
        Token::Address(self)
    }
}

impl Tokenize for U256 {
    fn from_token(token: Token) -> Result<Self, Error> {
        match token {
            Token::Uint(u256) => Ok(u256),
            _ => Err(Error::TypeMismatch),
        }
    }

    fn into_token(self) -> Token {
        Token::Uint(self)
    }
}

impl Tokenize for I256 {
    fn from_token(token: Token) -> Result<Self, Error> {
        match token {
            Token::Int(u256) => Ok(Self::from_raw(u256)),
            _ => Err(Error::TypeMismatch),
        }
    }

    fn into_token(self) -> Token {
        Token::Int(self.into_raw())
    }
}

impl Tokenize for TransactionHash {
    fn from_token(token: Token) -> Result<Self, Error>
    where
        Self: Sized,
    {
        let bytes = Bytes::from_token(token)?;
        Ok(Self(bytes.0))
    }

    fn into_token(self) -> Token {
        Bytes(self.0).into_token()
    }
}

macro_rules! uint_tokenize {
    ($int: ident, $token: ident) => {
        impl Tokenize for $int {
            fn from_token(token: Token) -> Result<Self, Error> {
                let u256 = match token {
                    Token::Uint(u256) => u256,
                    _ => return Err(Error::TypeMismatch),
                };
                u256.try_into().map_err(|_| Error::IntegerMismatch)
            }

            fn into_token(self) -> Token {
                Token::Uint(self.into())
            }
        }
    };
}

macro_rules! int_tokenize {
    ($int: ident, $token: ident) => {
        impl Tokenize for $int {
            fn from_token(token: Token) -> Result<Self, Error> {
                let u256 = match token {
                    Token::Int(u256) => u256,
                    _ => return Err(Error::TypeMismatch),
                };
                let i256 = I256::from_raw(u256);
                i256.try_into().map_err(|_| Error::IntegerMismatch)
            }

            fn into_token(self) -> Token {
                Token::Int(I256::from(self).into_raw())
            }
        }
    };
}

int_tokenize!(i8, Int);
int_tokenize!(i16, Int);
int_tokenize!(i32, Int);
int_tokenize!(i64, Int);
int_tokenize!(i128, Int);
uint_tokenize!(u8, Uint);
uint_tokenize!(u16, Uint);
uint_tokenize!(u32, Uint);
uint_tokenize!(u64, Uint);
uint_tokenize!(u128, Uint);

impl Tokenize for bool {
    fn from_token(token: Token) -> Result<Self, Error> {
        match token {
            Token::Bool(data) => Ok(data),
            _ => Err(Error::TypeMismatch),
        }
    }

    fn into_token(self) -> Token {
        Token::Bool(self)
    }
}

impl<T, const N: usize> Tokenize for [T; N]
where
    T: Tokenize,
{
    fn from_token(token: Token) -> Result<Self, Error>
    where
        Self: Sized,
    {
        let tokens = match token {
            Token::FixedArray(tokens) => tokens,
            _ => return Err(Error::TypeMismatch),
        };
        let arr_vec = tokens
            .into_iter()
            .map(T::from_token)
            .collect::<Result<ArrayVec<T, N>, _>>()?;
        arr_vec
            .into_inner()
            .map_err(|_| Error::FixedArrayLengthsMismatch)
    }

    fn into_token(self) -> Token {
        Token::FixedArray(
            ArrayVec::from(self)
                .into_iter()
                .map(T::into_token)
                .collect(),
        )
    }
}

impl<T> Tokenize for Vec<T>
where
    T: Tokenize,
{
    fn from_token(token: Token) -> Result<Self, Error> {
        match token {
            Token::Array(tokens) => tokens.into_iter().map(Tokenize::from_token).collect(),
            _ => Err(Error::TypeMismatch),
        }
    }

    fn into_token(self) -> Token {
        Token::Array(self.into_iter().map(Tokenize::into_token).collect())
    }
}

macro_rules! impl_single_tokenize_for_tuple {
    ($count: expr, $( $ty: ident : $no: tt, )*) => {
        impl<$($ty, )*> Tokenize for ($($ty,)*)
        where
            $($ty: Tokenize,)*
        {
            fn from_token(token: Token) -> Result<Self, Error>
            {
                let tokens = match token {
                    Token::Tuple(tokens) => tokens,
                    _ => return Err(Error::TypeMismatch),
                };
                if tokens.len() != $count {
                    return Err(Error::TupleLengthMismatch);
                }
                #[allow(unused_variables)]
                #[allow(unused_mut)]
                let mut drain = tokens.into_iter();
                Ok(($($ty::from_token(drain.next().unwrap())?,)*))
            }

            fn into_token(self) -> Token {
                Token::Tuple(vec![$(self.$no.into_token(),)*])
            }
        }
    }
}

impl_single_tokenize_for_tuple!(0,);
impl_single_tokenize_for_tuple!(1, A:0, );
impl_single_tokenize_for_tuple!(2, A:0, B:1, );
impl_single_tokenize_for_tuple!(3, A:0, B:1, C:2, );
impl_single_tokenize_for_tuple!(4, A:0, B:1, C:2, D:3, );
impl_single_tokenize_for_tuple!(5, A:0, B:1, C:2, D:3, E:4, );
impl_single_tokenize_for_tuple!(6, A:0, B:1, C:2, D:3, E:4, F:5, );
impl_single_tokenize_for_tuple!(7, A:0, B:1, C:2, D:3, E:4, F:5, G:6, );
impl_single_tokenize_for_tuple!(8, A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, );
impl_single_tokenize_for_tuple!(9, A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, );
impl_single_tokenize_for_tuple!(10, A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, );
impl_single_tokenize_for_tuple!(11, A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, );
impl_single_tokenize_for_tuple!(12, A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, L:11, );
impl_single_tokenize_for_tuple!(13, A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, L:11, M:12, );
impl_single_tokenize_for_tuple!(14, A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, L:11, M:12, N:13, );
impl_single_tokenize_for_tuple!(15, A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, L:11, M:12, N:13, O:14, );
impl_single_tokenize_for_tuple!(16, A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, L:11, M:12, N:13, O:14, P:15, );

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_single_tokenize_roundtrip<T>(value: T)
    where
        T: Tokenize + Clone + std::fmt::Debug + Eq,
    {
        assert_eq!(value, T::from_token(value.clone().into_token()).unwrap());
    }

    #[test]
    fn single_tokenize_roundtrip() {
        assert_single_tokenize_roundtrip(u8::MIN);
        assert_single_tokenize_roundtrip(u8::MAX);
        assert_single_tokenize_roundtrip(i8::MIN);
        assert_single_tokenize_roundtrip(i8::MAX);
        assert_single_tokenize_roundtrip(u16::MIN);
        assert_single_tokenize_roundtrip(i16::MAX);
        assert_single_tokenize_roundtrip(u32::MIN);
        assert_single_tokenize_roundtrip(i32::MAX);
        assert_single_tokenize_roundtrip(u64::MIN);
        assert_single_tokenize_roundtrip(i64::MAX);
        assert_single_tokenize_roundtrip(u128::MIN);
        assert_single_tokenize_roundtrip(i128::MAX);
        assert_single_tokenize_roundtrip(U256::zero());
        assert_single_tokenize_roundtrip(U256::MAX);
        assert_single_tokenize_roundtrip(I256::MIN);
        assert_single_tokenize_roundtrip(I256::MAX);
        assert_single_tokenize_roundtrip(false);
        assert_single_tokenize_roundtrip(true);
        assert_single_tokenize_roundtrip("abcd".to_string());
        assert_single_tokenize_roundtrip(vec![0u8, 1u8, 2u8]);
        assert_single_tokenize_roundtrip([0u8, 1u8, 2u8]);
        assert_single_tokenize_roundtrip(Bytes(vec![0u8, 1u8, 2u8]));
        assert_single_tokenize_roundtrip(Bytes([0u8, 1u8, 2u8]));
        assert_single_tokenize_roundtrip(Address::from_low_u64_be(42));
        assert_single_tokenize_roundtrip(TransactionHash::from_low_u64_be(42));
        assert_single_tokenize_roundtrip(());
        assert_single_tokenize_roundtrip((-1i8, 1i8));
        assert_single_tokenize_roundtrip([-1i8, 1i8]);
    }

    #[test]
    fn tokenize_bytes() {
        assert!(matches!([0u8].into_token(), Token::FixedArray(_)));
        assert!(matches!(vec![0u8].into_token(), Token::Array(_)));
        assert!(matches!(Bytes([0u8]).into_token(), Token::FixedBytes(_)));
        assert!(matches!(Bytes(vec![0u8]).into_token(), Token::Bytes(_)));
    }

    #[test]
    fn complex() {
        let rust = (vec![[(0u8, 1i8)]], false);
        let token = Token::Tuple(vec![
            Token::Array(vec![Token::FixedArray(vec![Token::Tuple(vec![
                Token::Uint(0.into()),
                Token::Int(1.into()),
            ])])]),
            Token::Bool(false),
        ]);
        assert_eq!(rust.clone().into_token(), token);
        assert_single_tokenize_roundtrip(rust);
    }

    #[test]
    fn i256_tokenization() {
        assert_eq!(I256::from(42).into_token(), 42i32.into_token());
        assert_eq!(I256::minus_one().into_token(), Token::Int(U256::MAX),);
        assert_eq!(
            I256::from_token(Token::Int(U256::MAX)).unwrap(),
            I256::minus_one()
        );

        assert_eq!(
            I256::from_token(42i32.into_token()).unwrap(),
            I256::from(42),
        );
    }
}
