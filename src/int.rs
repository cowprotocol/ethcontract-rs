//! This module contains an 256-bit signed integer implementation.

use crate::errors::ParseI256Error;
use std::fmt;
use std::str::{self, FromStr};
use web3::types::U256;

/// Compute the two's complement of a U256.
fn twos_complement(u: U256) -> U256 {
    let (twos_complement, _) = (u ^ U256::max_value()).overflowing_add(U256::one());
    twos_complement
}

/// Little-endian 256-bit signed integer.
#[derive(Clone, Copy, Default, Eq, Hash, PartialEq)]
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
    /// Creates an I256 from a sign and an absolute value.
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

    /// Returns an `i64` representing the sign of the number.
    fn signum64(self) -> i64 {
        let most_significant_word = (self.0).0[3] as i64;
        most_significant_word.signum()
    }

    /// Returns the sign of self.
    fn sign(self) -> Sign {
        match self.signum64() {
            1 | 0 => Sign::Positive,
            -1 => Sign::Negative,
            _ => unreachable!(),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use lazy_static::lazy_static;

    lazy_static! {
        static ref MIN_ABS: U256 = U256::from(1) << 255;
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
