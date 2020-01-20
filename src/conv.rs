//! Implements internal math utilities.

use web3::types::U256;

/// Lossy conversion from a `U256` to a `f64`.
pub fn u256_to_f64(value: U256) -> f64 {
    // NOTE: take 1 extra bit for rounding
    let exponent = value.bits().saturating_sub(54);
    let mantissa = (value >> U256::from(exponent)).as_u64();

    (mantissa as f64) * 2.0f64.powi(exponent as i32)
}

/// Lossy saturating conversion from a `f64` to a `U256`.
///
/// The conversion follows roughly the same rules as converting `f64` to other
/// primitive integer types. Namely, the conversion of `value: f64` behaves as
/// follows:
/// - `NaN` => `0`
/// - `(-∞, 0]` => `0`
/// - `(0, u256::MAX]` => `value as u256`
/// - `(u256::MAX, +∞)` => `u256::MAX`
pub fn f64_to_u256(value: f64) -> U256 {
    if value >= 1.0 {
        let bits = value.to_bits();
        // NOTE: Don't consider the sign or check that the subtraction will
        //   underflow since we already checked that the value is greater
        //   than 1.0.
        let exponent = ((bits >> 52) & 0x7ff) - 1023;
        let mantissa = (bits & 0x0f_ffff_ffff_ffff) | 0x10_0000_0000_0000;
        if exponent <= 52 {
            U256::from(mantissa >> (52 - exponent))
        } else if exponent >= 256 {
            U256::MAX
        } else {
            println!("{:b}", mantissa);
            U256::from(mantissa) << U256::from(exponent - 52)
        }
    } else {
        0.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64;

    #[test]
    #[allow(clippy::float_cmp)]
    fn convert_u256_to_f64() {
        assert_eq!(u256_to_f64(0.into()), 0.0);
        assert_eq!(u256_to_f64(42.into()), 42.0);
        assert_eq!(
            u256_to_f64(1_000_000_000_000_000_000u128.into()),
            1_000_000_000_000_000_000.0,
        );
    }

    #[test]
    #[allow(
        clippy::excessive_precision,
        clippy::float_cmp,
        clippy::unreadable_literal
    )]
    fn convert_u256_to_f64_precision_loss() {
        assert_eq!(
            u256_to_f64(u64::max_value().into()),
            u64::max_value() as f64,
        );
        assert_eq!(
            u256_to_f64(U256::MAX),
            115792089237316195423570985008687907853269984665640564039457584007913129639935.0,
        );
        assert_eq!(
            u256_to_f64(U256::MAX),
            115792089237316200000000000000000000000000000000000000000000000000000000000000.0,
        );
    }

    #[test]
    fn convert_f64_to_u256() {
        assert_eq!(f64_to_u256(0.0), 0.into());
        assert_eq!(f64_to_u256(13.37), 13.into());
        assert_eq!(f64_to_u256(42.0), 42.into());
        assert_eq!(f64_to_u256(999.999), 999.into());
        assert_eq!(
            f64_to_u256(1_000_000_000_000_000_000.0),
            1_000_000_000_000_000_000u128.into(),
        );
    }

    #[test]
    fn convert_f64_to_u256_large() {
        let value = U256::from(1) << U256::from(255);
        assert_eq!(
            f64_to_u256(
                format!("{}", value)
                    .parse::<f64>()
                    .expect("unexpected error parsing f64")
            ),
            value,
        );
    }

    #[test]
    #[allow(clippy::unreadable_literal)]
    fn convert_f64_to_u256_overflow() {
        assert_eq!(
            f64_to_u256(
                115792089237316200000000000000000000000000000000000000000000000000000000000000.0
            ),
            U256::MAX,
        );
        assert_eq!(
            f64_to_u256(
                999999999999999999999999999999999999999999999999999999999999999999999999999999.0
            ),
            U256::MAX,
        );
    }

    #[test]
    fn convert_f64_to_u256_non_normal() {
        assert_eq!(f64_to_u256(f64::EPSILON), 0.into());
        assert_eq!(f64_to_u256(f64::from_bits(0)), 0.into());
        assert_eq!(f64_to_u256(f64::NAN), 0.into());
        assert_eq!(f64_to_u256(f64::NEG_INFINITY), 0.into());
        assert_eq!(f64_to_u256(f64::INFINITY), U256::MAX);
    }
}
