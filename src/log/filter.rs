//! This module implements type-safe conversions into topics for filtering logs.

use crate::int::I256;
pub use ethcontract_common::abi::Topic;
use web3::types::{Address, H256, U256};

/// Trait for converting a value into an exact topic match.
pub trait IntoExactTopic {
    fn into_exact_topic(self) -> H256;
}

impl IntoExactTopic for H256 {
    fn into_exact_topic(self) -> H256 {
        self
    }
}

impl IntoExactTopic for Address {
    fn into_exact_topic(self) -> H256 {
        self.into()
    }
}

impl IntoExactTopic for U256 {
    fn into_exact_topic(self) -> H256 {
        let mut topic = H256::zero();
        self.to_big_endian(topic.as_mut());
        topic
    }
}

macro_rules! impl_uint {
    ($($t:ty),*) => {
        $(
            impl IntoExactTopic for $t {
                fn into_exact_topic(self) -> H256 {
                    U256::from(self).into_exact_topic()
                }
            }
        )*
    };
}

impl_uint!(u8, u16, u32, u64, u128, usize);

impl IntoExactTopic for I256 {
    fn into_exact_topic(self) -> H256 {
        self.into_raw().into_exact_topic()
    }
}

macro_rules! impl_int {
    ($($t:ty),*) => {
        $(
            impl IntoExactTopic for $t {
                fn into_exact_topic(self) -> H256 {
                    I256::from(self).into_exact_topic()
                }
            }
        )*
    };
}

impl_int!(i8, i16, i32, i64, i128, isize);

impl IntoExactTopic for bool {
    fn into_exact_topic(self) -> H256 {
        if self {
            H256::from_low_u64_be(1)
        } else {
            H256::zero()
        }
    }
}

// TODO(nlordell): Add support for complex types such as tuples, strings, bytes,
//   and arrays.

/// Trait for converting a value into a topic filter. This can either be for an
/// exact match, or be a collection of possilbe matches.
pub trait IntoTopic {
    fn into_topic(self) -> Topic<H256>;
}

impl<T: IntoTopic> IntoTopic for Option<T> {
    fn into_topic(self) -> Topic<H256> {
        match self {
            Some(value) => value.into_topic(),
            None => Topic::Any,
        }
    }
}
