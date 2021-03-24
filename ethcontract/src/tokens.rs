//! Tokenization related functionality allowing rust types to be mapped to solidity types.

// This file is based on https://github.com/tomusdrw/rust-web3/blob/e6d044a28458be9a3ee31108475d787e0440ce8b/src/contract/tokens.rs .
// Generated contract bindings should operate on native rust types for ease of use. To encode them
// with ethabi we need to map them to ethabi tokens. Tokenize does this for base types like
// u32 and compounds of other Tokenize in the form of vectors, arrays and tuples.
//
// This is complicated by `Vec<u8>` representing `Token::Bytes` (and `[u8; n]` `Token::FixedBytes`)
// preventing us from having a generic `impl<T: Tokenize> for Vec<T>` as this would lead to
// conflicting implementations. As a workaround we use an intermediate trait `TokenizeArray`
// that is implemented for all types that implement `Tokenize` except `Vec<u8>` and
// `[u8; n]` and then only implement `Tokenize` for vectors and arrays of
// `TokenizeArray`.
//
// The drawback is that if a solidity function actually used an array of u8 instead of bytes then we
// would not be able to interact with it. An alternative solution could be to use a Bytes new type
// but this makes calling those functions slightly more annoying.
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

impl Tokenize for Vec<u8> {
    fn from_token(token: Token) -> Result<Self, Error>
    where
        Self: Sized,
    {
        match token {
            Token::Bytes(bytes) => Ok(bytes),
            _ => Err(Error::TypeMismatch),
        }
    }

    fn into_token(self) -> Token {
        Token::Bytes(self)
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
        <[u8; 32]>::from_token(token).map(Self)
    }

    fn into_token(self) -> Token {
        self.0.into_token()
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

/// Marker trait for `Tokenize` types that are can tokenized to and from a `Token::Array` and
/// `Token:FixedArray`. This is everything except `u8` because `Vec<u8>` and `[u8; n]` directly
/// implement `Tokenize`.
pub trait TokenizeArray: Tokenize {}

macro_rules! single_tokenize_array {
    ($($type: ty,)*) => {
        $(
            impl TokenizeArray for $type {}
        )*
    };
}

single_tokenize_array! {
    String, Address, U256, I256, Vec<u8>, bool,
    i8, i16, i32, i64, i128, u16, u32, u64, u128,
}

impl<T: TokenizeArray> Tokenize for Vec<T> {
    fn from_token(token: Token) -> Result<Self, Error> {
        match token {
            Token::FixedArray(tokens) | Token::Array(tokens) => {
                tokens.into_iter().map(Tokenize::from_token).collect()
            }
            _ => Err(Error::TypeMismatch),
        }
    }

    fn into_token(self) -> Token {
        Token::Array(self.into_iter().map(Tokenize::into_token).collect())
    }
}

impl<T: TokenizeArray> TokenizeArray for Vec<T> {}

macro_rules! impl_fixed_types {
    ($num: expr) => {
        impl<T> Tokenize for [T; $num]
        where
            T: TokenizeArray,
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
                    .collect::<Result<ArrayVec<[T; $num]>, Error>>()?;
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

        impl<T> TokenizeArray for [T; $num] where T: TokenizeArray {}

        impl Tokenize for [u8; $num] {
            fn from_token(token: Token) -> Result<Self, Error> {
                match token {
                    Token::FixedBytes(bytes) => {
                        if bytes.len() != $num {
                            return Err(Error::TypeMismatch);
                        }

                        let mut arr = [0; $num];
                        arr.copy_from_slice(&bytes);
                        Ok(arr)
                    }
                    _ => Err(Error::TypeMismatch),
                }
            }

            fn into_token(self) -> Token {
                Token::FixedBytes(self.to_vec())
            }
        }

        impl TokenizeArray for [u8; $num] {}
    };
}

impl_fixed_types!(1);
impl_fixed_types!(2);
impl_fixed_types!(3);
impl_fixed_types!(4);
impl_fixed_types!(5);
impl_fixed_types!(6);
impl_fixed_types!(7);
impl_fixed_types!(8);
impl_fixed_types!(9);
impl_fixed_types!(10);
impl_fixed_types!(11);
impl_fixed_types!(12);
impl_fixed_types!(13);
impl_fixed_types!(14);
impl_fixed_types!(15);
impl_fixed_types!(16);
impl_fixed_types!(32);
impl_fixed_types!(64);
impl_fixed_types!(128);
impl_fixed_types!(256);
impl_fixed_types!(512);
impl_fixed_types!(1024);

macro_rules! impl_single_tokenize_for_tuple {
    ($count: expr, $( $ty: ident : $no: tt, )*) => {
        impl<$($ty, )*> TokenizeArray for ($($ty,)*)
        where
            $($ty: Tokenize,)*
        {}

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
        assert_single_tokenize_roundtrip(Address::from_low_u64_be(42));
        assert_single_tokenize_roundtrip(TransactionHash::from_low_u64_be(42));
        assert_single_tokenize_roundtrip(());
        assert_single_tokenize_roundtrip((-1i8, 1i8));
        assert_single_tokenize_roundtrip([-1i8, 1i8]);
    }

    #[test]
    fn tokenize_bytes() {
        assert!(matches!([0u8].into_token(), Token::FixedBytes(_)));
        assert!(matches!(vec![0u8].into_token(), Token::Bytes(_)));
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
