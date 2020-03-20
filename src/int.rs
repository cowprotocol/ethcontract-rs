//! This module contains an 256-bit signed integer implementation.

use crate::errors::{ParseI256Error, TryFromBigIntError};
use std::cmp;
use std::convert::{TryFrom, TryInto};
use std::fmt;
use std::i128;
use std::str::{self, FromStr};
use web3::types::U256;

/// Compute the two's complement of a U256.
fn twos_complement(u: U256) -> U256 {
    let (twos_complement, _) = (!u).overflowing_add(U256::one());
    twos_complement
}

/// Little-endian 256-bit signed integer.
#[derive(Clone, Copy, Default, Eq, Hash, Ord, PartialEq)]
pub struct I256(U256);

/// Enum to represent the sign of a 256-bit signed integer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Sign {
    /// Greater than or equal to zero.
    Positive,
    /// Less than zero.
    Negative,
}

impl I256 {
    /// Creates an I256 from a sign and an absolute value. Returns the value and
    /// a bool that is true if the conversion caused an overflow.
    fn overflowing_from_sign_and_abs(sign: Sign, abs: U256) -> (Self, bool) {
        let value = I256(match sign {
            Sign::Positive => abs,
            Sign::Negative => twos_complement(abs),
        });
        (value, value.sign() != sign)
    }

    /// Creates an I256 from an absolute value and a negative flag. Returns
    /// `None` if it would overflow an `I256`.
    fn checked_from_sign_and_abs(sign: Sign, abs: U256) -> Option<Self> {
        let (result, overflow) = I256::overflowing_from_sign_and_abs(sign, abs);
        if overflow {
            None
        } else {
            Some(result)
        }
    }

    /// Splits a I256 into its absolute value and negative flag.
    fn into_sign_and_abs(self) -> (Sign, U256) {
        let sign = self.sign();
        let abs = match sign {
            Sign::Positive => self.0,
            Sign::Negative => twos_complement(self.0),
        };
        (sign, abs)
    }

    /// Returns the sign of self.
    fn sign(self) -> Sign {
        let most_significant_word = (self.0).0[3];
        match most_significant_word & (1 << 63) {
            0 => Sign::Positive,
            _ => Sign::Negative,
        }
    }

    /// Coerces an unsigned integer into a signed one. If the unsigned integer
    /// is greater than the greater than or equal to `1 << 255`, then the result
    /// will overflow into a negative value.
    pub fn from_raw(raw: U256) -> Self {
        I256(raw)
    }

    /// Returns the signed integer as a unsigned integer. If the value of `self`
    /// negative, then the two's complement of its absolute value will be
    /// returned.
    pub fn into_raw(self) -> U256 {
        self.0
    }

    /// Conversion to i32
    pub fn low_i32(&self) -> i32 {
        self.0.low_u32() as _
    }

    /// Conversion to u32
    pub fn low_u32(&self) -> u32 {
        self.0.low_u32()
    }

    /// Conversion to i64
    pub fn low_i64(&self) -> i64 {
        self.0.low_u64() as _
    }

    /// Conversion to u64
    pub fn low_u64(&self) -> u64 {
        self.0.low_u64() as _
    }

    /// Conversion to i128
    pub fn low_i128(&self) -> i128 {
        self.0.low_u128() as _
    }

    /// Conversion to u128
    pub fn low_u128(&self) -> u128 {
        self.0.low_u128() as _
    }

    /// Conversion to i128
    pub fn low_isize(&self) -> isize {
        self.0.low_u64() as _
    }

    /// Conversion to usize
    pub fn low_usize(&self) -> usize {
        self.0.low_u64() as _
    }

    /// Conversion to i32 with overflow checking
    ///
    /// # Panics
    ///
    /// Panics if the number is outside the range [`i32::MIN`, `i32::MAX`].
    pub fn as_i32(&self) -> i32 {
        (*self).try_into().unwrap()
    }

    /// Conversion to u32 with overflow checking
    ///
    /// # Panics
    ///
    /// Panics if the number is outside the range [`0`, `u32::MAX`].
    pub fn as_u32(&self) -> u32 {
        (*self).try_into().unwrap()
    }

    /// Conversion to i64 with overflow checking
    ///
    /// # Panics
    ///
    /// Panics if the number is outside the range [`i64::MIN`, `i64::MAX`].
    pub fn as_i64(&self) -> i64 {
        (*self).try_into().unwrap()
    }

    /// Conversion to u64 with overflow checking
    ///
    /// # Panics
    ///
    /// Panics if the number is outside the range [`0`, `u64::MAX`].
    pub fn as_u64(&self) -> u64 {
        (*self).try_into().unwrap()
    }

    /// Conversion to i128 with overflow checking
    ///
    /// # Panics
    ///
    /// Panics if the number is outside the range [`i128::MIN`, `i128::MAX`].
    pub fn as_i128(&self) -> i128 {
        (*self).try_into().unwrap()
    }

    /// Conversion to u128 with overflow checking
    ///
    /// # Panics
    ///
    /// Panics if the number is outside the range [`0`, `u128::MAX`].
    pub fn as_u128(&self) -> u128 {
        (*self).try_into().unwrap()
    }

    /// Conversion to isize with overflow checking
    ///
    /// # Panics
    ///
    /// Panics if the number is outside the range [`isize::MIN`, `isize::MAX`].
    pub fn as_isize(&self) -> usize {
        (*self).try_into().unwrap()
    }

    /// Conversion to usize with overflow checking
    ///
    /// # Panics
    ///
    /// Panics if the number is outside the range [`0`, `usize::MAX`].
    pub fn as_usize(&self) -> usize {
        (*self).try_into().unwrap()
    }

    /// Convert from a decimal string.
    pub fn from_dec_str(value: &str) -> Result<Self, ParseI256Error> {
        let (sign, value) = match value.as_bytes().get(0) {
            Some(b'+') => (Sign::Positive, &value[1..]),
            Some(b'-') => (Sign::Negative, &value[1..]),
            _ => (Sign::Positive, value),
        };

        let abs = U256::from_dec_str(value)?;
        let result =
            I256::checked_from_sign_and_abs(sign, abs).ok_or(ParseI256Error::IntegerOverflow)?;

        Ok(result)
    }

    /// Convert from a hexadecimal string.
    pub fn from_hex_str(value: &str) -> Result<Self, ParseI256Error> {
        let (sign, value) = match value.as_bytes().get(0) {
            Some(b'+') => (Sign::Positive, &value[1..]),
            Some(b'-') => (Sign::Negative, &value[1..]),
            _ => (Sign::Positive, value),
        };

        // NOTE: Do the hex conversion here as `U256` implementation can panic.
        if value.len() > 64 {
            return Err(ParseI256Error::IntegerOverflow);
        }
        let mut abs = U256::zero();
        for (i, word) in value.as_bytes().rchunks(16).enumerate() {
            let word = str::from_utf8(word).map_err(|_| ParseI256Error::InvalidDigit)?;
            abs.0[i] = u64::from_str_radix(word, 16).map_err(|_| ParseI256Error::InvalidDigit)?;
        }

        let result =
            I256::checked_from_sign_and_abs(sign, abs).ok_or(ParseI256Error::IntegerOverflow)?;

        Ok(result)
    }
}
macro_rules! impl_from {
    ($( $t:ty ),*) => {
        $(
            impl From<$t> for I256 {
                fn from(value: $t) -> Self {
                    #[allow(unused_comparisons)]
                    I256(if value < 0 {
                        let abs = (u128::max_value() ^ (value as u128)).wrapping_add(1);
                        twos_complement(U256::from(abs))
                    } else {
                        U256::from(value)
                    })
                }
            }

            impl TryFrom<I256> for $t {
                type Error = TryFromBigIntError;

                fn try_from(value: I256) -> Result<Self, Self::Error> {
                    if value < I256::from(Self::min_value()) ||
                        value > I256::from(Self::max_value()) {
                        return Err(TryFromBigIntError);
                    }

                    Ok(value.0.low_u128() as _)
                }
            }
        )*
    };
}

impl_from!(i8, u8, i16, u16, i32, u32, i64, u64, i128, u128, isize, usize);

impl TryFrom<U256> for I256 {
    type Error = TryFromBigIntError;

    fn try_from(from: U256) -> Result<Self, Self::Error> {
        let value = I256(from);
        match value.sign() {
            Sign::Positive => Ok(value),
            Sign::Negative => Err(TryFromBigIntError),
        }
    }
}

impl TryFrom<I256> for U256 {
    type Error = TryFromBigIntError;

    fn try_from(value: I256) -> Result<Self, Self::Error> {
        match value.sign() {
            Sign::Positive => Ok(value.0),
            Sign::Negative => Err(TryFromBigIntError),
        }
    }
}

impl FromStr for I256 {
    type Err = ParseI256Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        I256::from_hex_str(value)
    }
}

impl fmt::Debug for I256 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for Sign {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match (self, f.sign_plus()) {
            (Sign::Positive, false) => Ok(()),
            (Sign::Positive, true) => write!(f, "+"),
            (Sign::Negative, _) => write!(f, "-"),
        }
    }
}

impl fmt::Display for I256 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let (sign, abs) = self.into_sign_and_abs();
        sign.fmt(f)?;
        write!(f, "{}", abs)
    }
}

impl fmt::LowerHex for I256 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let (sign, abs) = self.into_sign_and_abs();
        fmt::Display::fmt(&sign, f)?;
        write!(f, "{:x}", abs)
    }
}

impl fmt::UpperHex for I256 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let (sign, abs) = self.into_sign_and_abs();
        fmt::Display::fmt(&sign, f)?;

        // NOTE: Work around `U256: !UpperHex`.
        let mut buffer = format!("{:x}", abs);
        buffer.make_ascii_uppercase();
        write!(f, "{}", buffer)
    }
}

impl cmp::PartialOrd for I256 {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        // TODO(nlordell): Once subtraction is implemented:
        // self.saturating_sub(*other).signum64().partial_cmp(&0)

        use cmp::Ordering::*;
        use Sign::*;

        let ord = match (self.into_sign_and_abs(), other.into_sign_and_abs()) {
            ((Positive, _), (Negative, _)) => Greater,
            ((Negative, _), (Positive, _)) => Less,
            ((Positive, this), (Positive, other)) => this.cmp(&other),
            ((Negative, this), (Negative, other)) => other.cmp(&this),
        };

        Some(ord)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazy_static::lazy_static;

    lazy_static! {
        static ref MIN_ABS: U256 = U256::from(1) << 255;
    }

    #[test]
    #[allow(clippy::cognitive_complexity)]
    fn std_num_conversion() {
        let small_positive = I256::from(42);
        let small_negative = I256::from(-42);
        let large_positive =
            I256::from_dec_str("314159265358979323846264338327950288419716").unwrap();
        let large_negative =
            I256::from_dec_str("-314159265358979323846264338327950288419716").unwrap();
        let large_unsigned =
            U256::from_dec_str("314159265358979323846264338327950288419716").unwrap();

        macro_rules! assert_from {
            ($signed:ty, $unsigned:ty) => {
                assert_eq!(I256::from(-42 as $signed).to_string(), "-42");
                assert_eq!(I256::from(42 as $signed).to_string(), "42");
                assert_eq!(
                    I256::from(<$signed>::max_value()).to_string(),
                    <$signed>::max_value().to_string(),
                );
                assert_eq!(
                    I256::from(<$signed>::min_value()).to_string(),
                    <$signed>::min_value().to_string(),
                );

                assert_eq!(I256::from(42 as $unsigned).to_string(), "42");
                assert_eq!(
                    I256::from(<$unsigned>::max_value()).to_string(),
                    <$unsigned>::max_value().to_string(),
                );

                assert!(matches!(<$signed>::try_from(small_positive), Ok(42)));
                assert!(matches!(<$signed>::try_from(small_negative), Ok(-42)));
                assert!(matches!(<$signed>::try_from(large_positive), Err(_)));
                assert!(matches!(<$signed>::try_from(large_negative), Err(_)));

                assert!(matches!(<$unsigned>::try_from(small_positive), Ok(42)));
                assert!(matches!(<$unsigned>::try_from(small_negative), Err(_)));
                assert!(matches!(<$unsigned>::try_from(large_positive), Err(_)));
                assert!(matches!(<$unsigned>::try_from(large_negative), Err(_)));
            };
        }

        assert_eq!(I256::from(0).to_string(), "0");

        assert_from!(i8, u8);
        assert_from!(i16, u16);
        assert_from!(i32, u32);
        assert_from!(i64, u64);
        assert_from!(i128, u128);

        assert_eq!(I256::try_from(large_unsigned).unwrap(), large_positive);
        assert_eq!(U256::try_from(large_positive).unwrap(), large_unsigned);
        assert!(I256::try_from(U256::MAX).is_err());
        assert!(U256::try_from(small_negative).is_err());
        assert!(U256::try_from(large_negative).is_err());
    }

    #[test]
    fn parse_dec_str() {
        let unsigned = U256::from_dec_str("314159265358979323846264338327950288419716").unwrap();

        let value = I256::from_dec_str(&format!("-{}", unsigned)).unwrap();
        assert_eq!(value.into_sign_and_abs(), (Sign::Negative, unsigned));

        let value = I256::from_dec_str(&format!("{}", unsigned)).unwrap();
        assert_eq!(value.into_sign_and_abs(), (Sign::Positive, unsigned));

        let value = I256::from_dec_str(&format!("+{}", unsigned)).unwrap();
        assert_eq!(value.into_sign_and_abs(), (Sign::Positive, unsigned));

        let err = I256::from_dec_str("invalid string").unwrap_err();
        assert!(matches!(err, ParseI256Error::InvalidDigit));

        let err = I256::from_dec_str(&format!("1{}", U256::MAX)).unwrap_err();
        assert!(matches!(err, ParseI256Error::IntegerOverflow));

        let err = I256::from_dec_str(&format!("-{}", U256::MAX)).unwrap_err();
        assert!(matches!(err, ParseI256Error::IntegerOverflow));

        let value = I256::from_dec_str(&format!("-{}", *MIN_ABS)).unwrap();
        assert_eq!(value.into_sign_and_abs(), (Sign::Negative, *MIN_ABS));

        let err = I256::from_dec_str(&format!("{}", *MIN_ABS)).unwrap_err();
        assert!(matches!(err, ParseI256Error::IntegerOverflow));
    }

    #[test]
    fn parse_hex_str() {
        let unsigned = U256::from_dec_str("314159265358979323846264338327950288419716").unwrap();

        let value = I256::from_hex_str(&format!("-{:x}", unsigned)).unwrap();
        assert_eq!(value.into_sign_and_abs(), (Sign::Negative, unsigned));

        let value = I256::from_hex_str(&format!("{:x}", unsigned)).unwrap();
        assert_eq!(value.into_sign_and_abs(), (Sign::Positive, unsigned));

        let value = I256::from_hex_str(&format!("+{:x}", unsigned)).unwrap();
        assert_eq!(value.into_sign_and_abs(), (Sign::Positive, unsigned));

        let err = I256::from_hex_str("invalid string").unwrap_err();
        assert!(matches!(err, ParseI256Error::InvalidDigit));

        let err = I256::from_hex_str(&format!("1{:x}", U256::MAX)).unwrap_err();
        assert!(matches!(err, ParseI256Error::IntegerOverflow));

        let err = I256::from_hex_str(&format!("-{:x}", U256::MAX)).unwrap_err();
        assert!(matches!(err, ParseI256Error::IntegerOverflow));

        let value = I256::from_hex_str(&format!("-{:x}", *MIN_ABS)).unwrap();
        assert_eq!(value.into_sign_and_abs(), (Sign::Negative, *MIN_ABS));

        let err = I256::from_hex_str(&format!("{:x}", *MIN_ABS)).unwrap_err();
        assert!(matches!(err, ParseI256Error::IntegerOverflow));
    }

    #[test]
    fn formatting() {
        let unsigned = U256::from_dec_str("314159265358979323846264338327950288419716").unwrap();

        // TODO(nlordell): Simplify once negation is implemented.
        let positive = I256::checked_from_sign_and_abs(Sign::Positive, unsigned).unwrap();
        let negative = I256::checked_from_sign_and_abs(Sign::Negative, unsigned).unwrap();

        assert_eq!(format!("{}", positive), format!("{}", unsigned));
        assert_eq!(format!("{}", negative), format!("-{}", unsigned));
        assert_eq!(format!("{:+}", positive), format!("+{}", unsigned));
        assert_eq!(format!("{:+}", negative), format!("-{}", unsigned));

        assert_eq!(format!("{:x}", positive), format!("{:x}", unsigned));
        assert_eq!(format!("{:x}", negative), format!("-{:x}", unsigned));
        assert_eq!(format!("{:+x}", positive), format!("+{:x}", unsigned));
        assert_eq!(format!("{:+x}", negative), format!("-{:x}", unsigned));

        assert_eq!(
            format!("{:X}", positive),
            format!("{:x}", unsigned).to_uppercase()
        );
        assert_eq!(
            format!("{:X}", negative),
            format!("-{:x}", unsigned).to_uppercase()
        );
        assert_eq!(
            format!("{:+X}", positive),
            format!("+{:x}", unsigned).to_uppercase()
        );
        assert_eq!(
            format!("{:+X}", negative),
            format!("-{:x}", unsigned).to_uppercase()
        );
    }
}
