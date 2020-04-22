//! This module implements compatibility conversions between `ethabi@9.0` which
//! is used by `web3` and `ethabi@11.0` which is the latest. Unfortunately there
//! are important fixes in the new version so this compatibility layer is
//! necessary until a new `web3` version with the latest `ethabi` is released.

use ethcontract_common::abi as ethabi_12_0;

/// A compatibility trait implemented for converting between `ethabi@9.0` and
/// `ethabi@11.0` types.
pub trait AbiCompat {
    /// The equivalent type from the other crate version.
    type Compat;

    /// Convert `self` into the its other crate version equivalent.
    fn compat(self) -> Self::Compat;
}

impl AbiCompat for ethabi_9_0::Error {
    type Compat = ethabi_12_0::Error;

    fn compat(self) -> Self::Compat {
        let ethabi_9_0::Error(kind, _) = self;
        match kind {
            ethabi_9_0::ErrorKind::Msg(err) => ethabi_12_0::Error::Other(err),
            ethabi_9_0::ErrorKind::SerdeJson(err) => ethabi_12_0::Error::SerdeJson(err),
            ethabi_9_0::ErrorKind::ParseInt(err) => ethabi_12_0::Error::ParseInt(err),
            ethabi_9_0::ErrorKind::Utf8(err) => ethabi_12_0::Error::Utf8(err),
            ethabi_9_0::ErrorKind::Hex(err) => ethabi_12_0::Error::Hex(err),
            ethabi_9_0::ErrorKind::InvalidName(name) => ethabi_12_0::Error::InvalidName(name),
            ethabi_9_0::ErrorKind::InvalidData => ethabi_12_0::Error::InvalidData,

            // NOTE: There is a `__Nonexaustive` variant that should never be
            // contructed, so the extra match arm is required here.
            _ => unreachable!(),
        }
    }
}

impl AbiCompat for ethabi_9_0::Token {
    type Compat = ethabi_12_0::Token;

    fn compat(self) -> Self::Compat {
        match self {
            ethabi_9_0::Token::Address(value) => ethabi_12_0::Token::Address(value.compat()),
            ethabi_9_0::Token::FixedBytes(value) => ethabi_12_0::Token::FixedBytes(value),
            ethabi_9_0::Token::Bytes(value) => ethabi_12_0::Token::Bytes(value),
            ethabi_9_0::Token::Int(value) => ethabi_12_0::Token::Int(value.compat()),
            ethabi_9_0::Token::Uint(value) => ethabi_12_0::Token::Uint(value.compat()),
            ethabi_9_0::Token::Bool(value) => ethabi_12_0::Token::Bool(value),
            ethabi_9_0::Token::String(value) => ethabi_12_0::Token::String(value),
            ethabi_9_0::Token::FixedArray(value) => ethabi_12_0::Token::FixedArray(value.compat()),
            ethabi_9_0::Token::Array(value) => ethabi_12_0::Token::Array(value.compat()),
        }
    }
}

impl AbiCompat for Vec<ethabi_9_0::Token> {
    type Compat = Vec<ethabi_12_0::Token>;

    fn compat(self) -> Self::Compat {
        let mut tokens = Vec::with_capacity(self.len());
        for token in self {
            tokens.push(token.compat());
        }
        tokens
    }
}

impl AbiCompat for ethabi_12_0::Token {
    type Compat = Option<ethabi_9_0::Token>;

    fn compat(self) -> Self::Compat {
        match self {
            ethabi_12_0::Token::Address(value) => Some(ethabi_9_0::Token::Address(value.compat())),
            ethabi_12_0::Token::FixedBytes(value) => Some(ethabi_9_0::Token::FixedBytes(value)),
            ethabi_12_0::Token::Bytes(value) => Some(ethabi_9_0::Token::Bytes(value)),
            ethabi_12_0::Token::Int(value) => Some(ethabi_9_0::Token::Int(value.compat())),
            ethabi_12_0::Token::Uint(value) => Some(ethabi_9_0::Token::Uint(value.compat())),
            ethabi_12_0::Token::Bool(value) => Some(ethabi_9_0::Token::Bool(value)),
            ethabi_12_0::Token::String(value) => Some(ethabi_9_0::Token::String(value)),
            ethabi_12_0::Token::FixedArray(value) => {
                Some(ethabi_9_0::Token::FixedArray(value.compat()?))
            }
            ethabi_12_0::Token::Array(value) => Some(ethabi_9_0::Token::Array(value.compat()?)),
            ethabi_12_0::Token::Tuple(_) => None,
        }
    }
}

impl AbiCompat for Vec<ethabi_12_0::Token> {
    type Compat = Option<Vec<ethabi_9_0::Token>>;

    fn compat(self) -> Self::Compat {
        let mut tokens = Vec::with_capacity(self.len());
        for token in self {
            tokens.push(token.compat()?);
        }
        Some(tokens)
    }
}

impl<T: AbiCompat> AbiCompat for ethabi_12_0::Topic<T> {
    type Compat = ethabi_9_0::Topic<T::Compat>;

    fn compat(self) -> Self::Compat {
        match self {
            ethabi_12_0::Topic::Any => ethabi_9_0::Topic::Any,
            ethabi_12_0::Topic::OneOf(values) => {
                let values = values.into_iter().map(T::compat).collect();
                ethabi_9_0::Topic::OneOf(values)
            }
            ethabi_12_0::Topic::This(value) => ethabi_9_0::Topic::This(value.compat()),
        }
    }
}

impl AbiCompat for ethabi_12_0::TopicFilter {
    type Compat = ethabi_9_0::TopicFilter;

    fn compat(self) -> Self::Compat {
        ethabi_9_0::TopicFilter {
            topic0: self.topic0.compat(),
            topic1: self.topic1.compat(),
            topic2: self.topic2.compat(),
            topic3: self.topic3.compat(),
        }
    }
}

macro_rules! impl_primitive_compat {
    ($($prim:ident),*) => {$(
        impl_primitive_compat!(__impl: ethabi_12_0::$prim, ethabi_9_0::$prim);
        impl_primitive_compat!(__impl: ethabi_9_0::$prim, ethabi_12_0::$prim);
    )*};
    (__impl: $from:ty, $to:ty) => {
        impl AbiCompat for $from {
            type Compat = $to;

            fn compat(self) -> Self::Compat {
                // NOTE: Work around not being able to construct tuple structs
                //   from their aliases.
                let mut result = Self::Compat::default();
                result.0 = self.0;
                result
            }
        }

        impl AbiCompat for Vec<$from> {
            type Compat = Vec<$to>;

            fn compat(self) -> Self::Compat {
                self.into_iter().map(<$from>::compat).collect()
            }
        }
    };
}

impl_primitive_compat!(Address, Hash, Uint);
