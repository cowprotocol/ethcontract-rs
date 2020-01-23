//! Implementation of gas price estimation.

use crate::conv;
use crate::future::CompatCallFuture;
use futures::compat::Future01CompatExt;
use futures::future::OptionFuture;
use pin_project::{pin_project, project};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use web3::api::Web3;
use web3::error::Error as Web3Error;
use web3::types::U256;
use web3::Transport;

/// The gas price setting to use.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GasPrice {
    /// The standard estimated gas price from the node, this is usually the
    /// median gas price from the last few blocks. This is the default gas price
    /// used by transactions.
    Standard,
    /// A factor of the estimated gas price from the node. `GasPrice::Standard`
    /// is similar to `GasPrice::Scaled(1.0)` but because of how the scaling is
    /// calculated, `GasPrice::Scaled(1.0)` can lead to some rounding errors
    /// caused by converting the estimated gas price from the node to a `f64`
    /// and back.
    Scaled(f64),
    /// Specify a specific gas price to use for the transaction. This will cause
    /// the transaction `SendFuture` to not query the node for a gas price
    /// estimation.
    Value(U256),
}

impl GasPrice {
    /// A low gas price. Using this may result in long confirmation times for
    /// transactions, or the transactions not being mined at all.
    pub fn low() -> Self {
        GasPrice::Scaled(0.8)
    }

    /// A high gas price that usually results in faster mining times.
    /// transactions, or the transactions not being mined at all.
    pub fn high() -> Self {
        GasPrice::Scaled(6.0)
    }

    /// Returns `Some(value)` if the gas price is explicitly specified, `None`
    /// otherwise.
    pub fn value(&self) -> Option<U256> {
        match self {
            GasPrice::Value(value) => Some(*value),
            _ => None,
        }
    }

    /// Resolves the gas price into a value. Returns a future that resolves once
    /// the gas price is calculated as this may require contacting the node for
    /// gas price estimates in the case of `GasPrice::Standard` and
    /// `GasPrice::Scaled`.
    pub fn resolve<T: Transport>(web3: &Web3<T>, gas_price: Self) -> ResolveGasPriceFuture<T> {
        ResolveGasPriceFuture::new(web3, gas_price)
    }

    /// Resolves the gas price into an `Option<U256>` intendend to be used by a
    /// `TransactionRequest`. Note that `TransactionRequest`s gas price default
    /// to the node's estimate (i.e. `GasPrice::Standard`) when omitted, so this
    /// allows for a small optimization by foregoing a JSON RPC request.
    pub fn resolve_for_transaction_request<T: Transport>(
        web3: &Web3<T>,
        gas_price: Option<Self>,
    ) -> ResolveTransactionRequestGasPriceFuture<T> {
        let future = match gas_price {
            None | Some(GasPrice::Standard) => None,
            Some(gas_price) => Some(GasPrice::resolve(web3, gas_price)),
        };
        future.into()
    }
}

impl Default for GasPrice {
    fn default() -> Self {
        GasPrice::Standard
    }
}

impl From<U256> for GasPrice {
    fn from(value: U256) -> Self {
        GasPrice::Value(value)
    }
}

macro_rules! impl_gas_price_from_integer {
    ($($t:ty),* $(,)?) => {
        $(
            impl From<$t> for GasPrice {
                fn from(value: $t) -> Self {
                    GasPrice::Value(value.into())
                }
            }
        )*
    };
}

impl_gas_price_from_integer! {
    i8, i16, i32, i64, i128, isize,
    u8, u16, u32, u64, u128, usize,
}

/// Future for resolving gas prices.
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[pin_project]
pub struct ResolveGasPriceFuture<T: Transport> {
    /// The state of the future.
    #[pin]
    state: ResolveGasPriceState<T>,
}

/// The state of the `ResolveGasPriceFuture`. This type is used to not leak
/// internal implementation details.
#[pin_project]
enum ResolveGasPriceState<T: Transport> {
    /// The gas price is known before hand.
    Ready(U256),
    /// The gas price estimate is being queried with the node. Optinally, the
    /// gas price will be scaled once retrieved.
    Estimating(#[pin] CompatCallFuture<T, U256>, Option<f64>),
}

impl<T: Transport> ResolveGasPriceFuture<T> {
    /// Creates a new future that resolves once the gas price value is computed.
    pub fn new(web3: &Web3<T>, gas_price: GasPrice) -> Self {
        let state = match gas_price {
            GasPrice::Standard => {
                ResolveGasPriceState::Estimating(web3.eth().gas_price().compat(), None)
            }
            GasPrice::Scaled(factor) => {
                ResolveGasPriceState::Estimating(web3.eth().gas_price().compat(), Some(factor))
            }
            GasPrice::Value(value) => ResolveGasPriceState::Ready(value),
        };

        ResolveGasPriceFuture { state }
    }
}

impl<T: Transport> Future for ResolveGasPriceFuture<T> {
    type Output = Result<U256, Web3Error>;

    #[project]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = self.project();

        #[project]
        match this.state.project() {
            ResolveGasPriceState::Ready(value) => Poll::Ready(Ok(*value)),
            ResolveGasPriceState::Estimating(gas_price, factor) => {
                gas_price.poll(cx).map(|gas_price| {
                    if let Some(factor) = factor {
                        Ok(scale_gas_price(gas_price?, *factor))
                    } else {
                        gas_price
                    }
                })
            }
        }
    }
}

/// A type alias for an optional `ResolveGasPriceFuture` when resolving the gas
/// price for a `TransactionRequest`.
pub type ResolveTransactionRequestGasPriceFuture<T> = OptionFuture<ResolveGasPriceFuture<T>>;

/// Apply a scaling factor to a gas price.
fn scale_gas_price(gas_price: U256, factor: f64) -> U256 {
    // NOTE: U256 does not support floating point multiplication we have to
    //   convert everything to floats to multiply the factor and then convert
    //   back. We are OK with the loss of precision here.
    let gas_price_f = conv::u256_to_f64(gas_price);
    conv::f64_to_u256(gas_price_f * factor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::prelude::*;

    #[test]
    fn gas_price_scalling() {
        assert_eq!(scale_gas_price(1_000_000.into(), 2.0), 2_000_000.into());
        assert_eq!(scale_gas_price(1_000_000.into(), 1.5), 1_500_000.into());
        assert_eq!(scale_gas_price(U256::MAX, 2.0), U256::MAX);
    }

    #[test]
    fn resolve_gas_price() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let gas_price = U256::from(1_000_000);

        transport.add_response(json!(gas_price));
        assert_eq!(
            GasPrice::resolve(&web3, GasPrice::Standard)
                .immediate()
                .expect("error resolving gas price"),
            gas_price
        );
        transport.assert_request("eth_gasPrice", &[]);
        transport.assert_no_more_requests();

        transport.add_response(json!(gas_price));
        assert_eq!(
            GasPrice::resolve(&web3, GasPrice::Scaled(2.0))
                .immediate()
                .expect("error resolving gas price"),
            gas_price * 2
        );
        transport.assert_request("eth_gasPrice", &[]);
        transport.assert_no_more_requests();

        assert_eq!(
            GasPrice::resolve(&web3, GasPrice::Value(gas_price))
                .immediate()
                .expect("error resolving gas price"),
            gas_price
        );
        transport.assert_no_more_requests();
    }

    #[test]
    fn resolve_gas_price_for_transaction_request() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let gas_price = U256::from(1_000_000);

        assert_eq!(
            GasPrice::resolve_for_transaction_request(&web3, None)
                .immediate()
                .transpose()
                .expect("error resolving gas price"),
            None
        );
        transport.assert_no_more_requests();

        assert_eq!(
            GasPrice::resolve_for_transaction_request(&web3, Some(GasPrice::Standard))
                .immediate()
                .transpose()
                .expect("error resolving gas price"),
            None
        );
        transport.assert_no_more_requests();

        transport.add_response(json!(gas_price));
        assert_eq!(
            GasPrice::resolve_for_transaction_request(&web3, Some(GasPrice::Scaled(2.0)))
                .immediate()
                .transpose()
                .expect("error resolving gas price"),
            Some(gas_price * 2),
        );
        transport.assert_request("eth_gasPrice", &[]);
        transport.assert_no_more_requests();

        assert_eq!(
            GasPrice::resolve_for_transaction_request(&web3, Some(GasPrice::Value(gas_price)))
                .immediate()
                .transpose()
                .expect("error resolving gas price"),
            Some(gas_price)
        );
        transport.assert_no_more_requests();
    }
}
