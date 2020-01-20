//! This module implements transaction finalization from partial transaction
//! parameters. It provides futures for building `TransactionRequest` instances
//! and raw `Bytes` transactions from partial transaction parameters, where the
//! remaining parameters are queried from the node before finalizing the
//! transaction.

use crate::errors::ExecutionError;
use crate::future::{CompatCallFuture, MaybeReady};
use crate::sign::TransactionData;
use crate::transaction::{Account, EstimateGasFuture, GasPrice, Transaction, TransactionBuilder};
use ethsign::{Protected, SecretKey};
use futures::compat::Future01CompatExt;
use futures::future::{self, Join, OptionFuture, TryJoin4};
use futures::ready;
use pin_project::{pin_project, project};
use std::future::Future;
use std::pin::Pin;
use std::str;
use std::task::{Context, Poll};
use web3::api::Web3;
use web3::helpers::CallFuture;
use web3::types::{
    Address, Bytes, CallRequest, RawTransaction, TransactionCondition, TransactionRequest, U256,
    U64,
};
use web3::Transport;

/// A partial transaction object that will get finalized into a transaction
/// request or a raw transaction.
#[derive(Clone, Debug, Default)]
pub struct PartialTransaction {
    /// The receiver of the transaction.
    pub to: Option<Address>,
    /// The amount of gas to use for the transaction.
    pub gas: Option<U256>,
    /// The gas price to use for the transaction.
    pub gas_price: Option<GasPrice>,
    /// The ETH value to send with the transaction.
    pub value: Option<U256>,
    /// The data for the transaction.
    pub data: Option<Bytes>,
    /// The transaction nonce.
    pub nonce: Option<U256>,
}

impl PartialTransaction {
    /// Converts a partial transaction into a transaction request by specifying
    /// the missing parameters.
    fn into_request(
        self,
        from: Address,
        condition: Option<TransactionCondition>,
        gas_price_estimate: Option<U256>,
    ) -> TransactionRequest {
        let gas_price = self.gas_price.unwrap_or_default();
        let gas_price = if let Some(gas_price_estimate) = gas_price_estimate {
            Some(gas_price.get_price(gas_price_estimate))
        } else {
            gas_price.value()
        };

        TransactionRequest {
            from,
            to: self.to,
            gas: self.gas,
            gas_price,
            value: self.value,
            data: self.data,
            nonce: self.nonce,
            condition,
        }
    }
}

/// Future for preparing a transaction.
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[pin_project]
pub enum BuildFuture<T: Transport> {
    /// Locally signed transaction. Produces a `Transaction::Request` result.
    Local(#[pin] LocalBuildFuture<T>),
    /// Locally signed transaction with locked account. Produces a
    /// `Transaction::Raw` result.
    Locked(#[pin] LockedBuildFuture<T>),
    /// Offline signed transaction. Produces a `Transaction::Raw` result.
    Offline(#[pin] OfflineBuildFuture<T>),
}

impl<T: Transport> BuildFuture<T> {
    /// Create an instance from a `TransactionBuilder`.
    pub fn from_builder(builder: TransactionBuilder<T>) -> Self {
        let tx = PartialTransaction {
            to: builder.to,
            gas: builder.gas,
            gas_price: builder.gas_price,
            value: builder.value,
            data: builder.data,
            nonce: builder.nonce,
        };

        match builder.from {
            None => BuildFuture::Local(LocalBuildFuture::new(&builder.web3, tx, None, None)),
            Some(Account::Local(from, condition)) => BuildFuture::Local(LocalBuildFuture::new(
                &builder.web3,
                tx,
                Some(from),
                condition,
            )),
            Some(Account::Locked(from, password, condition)) => BuildFuture::Locked(
                LockedBuildFuture::new(builder.web3, tx, from, password, condition),
            ),
            Some(Account::Offline(key, chain_id)) => {
                BuildFuture::Offline(OfflineBuildFuture::new(&builder.web3, tx, key, chain_id))
            }
        }
    }
}

impl<T: Transport> Future for BuildFuture<T> {
    type Output = Result<Transaction, ExecutionError>;

    #[project]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        #[project]
        match self.project() {
            BuildFuture::Local(local) => local
                .poll(cx)
                .map(|request| Ok(Transaction::Request(request?))),
            BuildFuture::Locked(locked) => locked.poll(cx).map(|raw| Ok(Transaction::Raw(raw?))),
            BuildFuture::Offline(offline) => offline.poll(cx).map(|raw| Ok(Transaction::Raw(raw?))),
        }
    }
}

macro_rules! maybe {
    ($o:expr, $c:expr) => {
        match $o {
            Some(v) => MaybeReady::ready(Ok(v)),
            None => MaybeReady::future($c),
        }
    };
}

/// Type alias for a call future that might already be resolved.
type MaybeCallFuture<T, R> = MaybeReady<CompatCallFuture<T, R>>;

/// Type alias for an optional call future.
type OptionCallFuture<T, R> = OptionFuture<CompatCallFuture<T, R>>;

/// Type alias for future retrieving default local account parameters.
type LocalParamsFuture<T> = Join<MaybeCallFuture<T, Vec<Address>>, OptionCallFuture<T, U256>>;

/// A future for building a locally signed transaction.
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[pin_project]
pub struct LocalBuildFuture<T: Transport> {
    /// The partial transaction option with the optional condition.
    tx: Option<(PartialTransaction, Option<TransactionCondition>)>,
    /// The inner future for retrieving the list of accounts on the node and
    /// gas price estimation.
    #[pin]
    params: LocalParamsFuture<T>,
}

impl<T: Transport> LocalBuildFuture<T> {
    /// Create a new future for building a locally singed transaction request
    /// from a partial transaction object and account information.
    pub fn new(
        web3: &Web3<T>,
        tx: PartialTransaction,
        from: Option<Address>,
        condition: Option<TransactionCondition>,
    ) -> Self {
        let eth = web3.eth();
        let accounts = maybe!(from.map(|from| vec![from]), eth.accounts().compat());
        let gas_price_estimate = match tx.gas_price {
            Some(GasPrice::Factor(_)) => Some(eth.gas_price().compat()),
            _ => None,
        };

        LocalBuildFuture {
            tx: Some((tx, condition)),
            params: future::join(accounts, gas_price_estimate.into()),
        }
    }
}

impl<T: Transport> Future for LocalBuildFuture<T> {
    type Output = Result<TransactionRequest, ExecutionError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut this = self.project();
        this.params
            .as_mut()
            .poll(cx)
            .map(|(accounts, gas_price_estimate)| {
                let (tx, condition) = this.tx.take().expect("future polled more than once");

                let from = accounts?.get(0).copied().unwrap_or_default();
                let gas_price_estimate = gas_price_estimate.transpose()?;

                Ok(tx.into_request(from, condition, gas_price_estimate))
            })
    }
}

/// A future for building a locally signed transaction with a locked account.
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[pin_project]
pub struct LockedBuildFuture<T: Transport> {
    /// The underlying web3 provider.
    web3: Web3<T>,
    /// The password used for signing.
    password: Protected,
    /// The state of the build future.
    #[pin]
    state: LockedBuildState<T>,
}

/// The state of the `LockedBuildFuture`.
#[pin_project]
enum LockedBuildState<T: Transport> {
    /// Preparing the transaction request.
    Preparing(#[pin] LocalBuildFuture<T>),
    /// Signing the transaction request.
    Signing(#[pin] CompatCallFuture<T, RawTransaction>),
}

impl<T: Transport> LockedBuildFuture<T> {
    /// Create a new future for building a locally singed transaction request
    /// from a partial transaction object and account information.
    pub fn new(
        web3: Web3<T>,
        tx: PartialTransaction,
        from: Address,
        password: Protected,
        condition: Option<TransactionCondition>,
    ) -> Self {
        let tx_request = LocalBuildFuture::new(&web3, tx, Some(from), condition);
        LockedBuildFuture {
            web3,
            password,
            state: LockedBuildState::Preparing(tx_request),
        }
    }
}

impl<T: Transport> Future for LockedBuildFuture<T> {
    type Output = Result<Bytes, ExecutionError>;

    #[project]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut this = self.project();

        loop {
            #[project]
            let next_state = match this.state.as_mut().project() {
                LockedBuildState::Preparing(tx_request) => {
                    let tx_request = match ready!(tx_request.poll(cx)) {
                        Ok(tx_request) => tx_request,
                        Err(err) => return Poll::Ready(Err(err)),
                    };

                    let password = unsafe { str::from_utf8_unchecked(this.password.as_ref()) };
                    let sign = this
                        .web3
                        .personal()
                        .sign_transaction(tx_request, password)
                        .compat();

                    LockedBuildState::Signing(sign)
                }
                LockedBuildState::Signing(sign) => return sign.poll(cx).map(|raw| Ok(raw?.raw)),
            };

            *this.state = next_state;
        }
    }
}

/// Type alias for future retrieving the optional parameters that may not have
/// been specified by the transaction builder but are required for signing.
type OfflineParamsFuture<T> = TryJoin4<
    MaybeCallFuture<T, U256>,
    MaybeCallFuture<T, U256>,
    MaybeCallFuture<T, U256>,
    MaybeCallFuture<T, U64>,
>;

/// A future for building a offline signed transaction.
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[pin_project]
pub struct OfflineBuildFuture<T: Transport> {
    /// The private key to use for signing.
    key: SecretKey,
    /// The gas price to use.
    gas_price: GasPrice,
    /// The recepient address.
    to: Address,
    /// The ETH value to be sent with the transaction.
    value: U256,
    /// The ABI encoded call parameters,
    data: Bytes,
    /// Future for retrieving gas, gas price, nonce and chain ID when they
    /// where not specified.
    #[pin]
    params: OfflineParamsFuture<T>,
}

impl<T: Transport> OfflineBuildFuture<T> {
    /// Create a new future for building a locally singed transaction request
    /// from a partial transaction object and account information.
    pub fn new(
        web3: &Web3<T>,
        tx: PartialTransaction,
        key: SecretKey,
        chain_id: Option<u64>,
    ) -> Self {
        let from = key.public().address().into();
        let gas_price = tx.gas_price.unwrap_or_default();
        let to = tx.to.unwrap_or_else(Address::zero);
        let transport = web3.transport();

        let eth = web3.eth();

        let gas = maybe!(
            tx.gas,
            EstimateGasFuture::from_request(
                eth.clone(),
                CallRequest {
                    from: Some(from),
                    to,
                    gas: None,
                    gas_price: None,
                    value: tx.value,
                    data: tx.data.clone(),
                }
            )
            .0
        );
        let gas_price_estimate = maybe!(gas_price.value(), eth.gas_price().compat());
        let nonce = maybe!(tx.nonce, eth.transaction_count(from, None).compat());
        let chain_id = maybe!(
            chain_id.map(U64::from),
            CallFuture::new(transport.execute("eth_chainId", vec![])).compat()
        );

        OfflineBuildFuture {
            key,
            gas_price,
            to,
            value: tx.value.unwrap_or_else(U256::zero),
            data: tx.data.unwrap_or_else(Bytes::default),
            params: future::try_join4(gas, gas_price_estimate, nonce, chain_id),
        }
    }
}

impl<T: Transport> Future for OfflineBuildFuture<T> {
    type Output = Result<Bytes, ExecutionError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut this = self.project();
        this.params.as_mut().poll(cx).map(|params| {
            let (gas, gas_price_estimate, nonce, chain_id) = params?;
            let gas_price = this.gas_price.get_price(gas_price_estimate);

            let tx = TransactionData {
                nonce,
                gas_price,
                gas,
                to: *this.to,
                value: *this.value,
                data: &this.data,
            };
            let raw = tx.sign(&this.key, Some(chain_id.as_u64()))?;

            Ok(raw)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::prelude::*;

    #[test]
    fn tx_build_local_default_account() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let accounts = [
            addr!("0x9876543210987654321098765432109876543210"),
            addr!("0x1111111111111111111111111111111111111111"),
            addr!("0x2222222222222222222222222222222222222222"),
        ];

        transport.add_response(json!(accounts)); // get accounts
        let tx = LocalBuildFuture::new(&web3, PartialTransaction::default(), None, None)
            .immediate()
            .expect("get accounts success");

        transport.assert_request("eth_accounts", &[]);
        transport.assert_no_more_requests();

        assert_eq!(tx.from, accounts[0]);
        assert_eq!(tx.gas_price, None);
    }

    #[test]
    fn tx_build_local_default_account_with_extra_gas_price() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let accounts = [
            addr!("0x9876543210987654321098765432109876543210"),
            addr!("0x1111111111111111111111111111111111111111"),
            addr!("0x2222222222222222222222222222222222222222"),
        ];

        transport.add_response(json!(accounts)); // get accounts
        transport.add_response(json!("0x42")); // gas price
        let tx = LocalBuildFuture::new(
            &web3,
            PartialTransaction {
                gas_price: Some(GasPrice::Factor(2.0)),
                ..Default::default()
            },
            None,
            None,
        )
        .immediate()
        .expect("get accounts success");

        transport.assert_request("eth_accounts", &[]);
        transport.assert_request("eth_gasPrice", &[]);
        transport.assert_no_more_requests();

        assert_eq!(tx.from, accounts[0]);
        assert_eq!(tx.gas_price, Some(U256::from(0x42 * 2)));
    }

    #[test]
    fn tx_build_local_with_extra_gas_price() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let from = addr!("0xffffffffffffffffffffffffffffffffffffffff");

        transport.add_response(json!("0x42")); // gas price
        let tx = LocalBuildFuture::new(
            &web3,
            PartialTransaction {
                gas_price: Some(GasPrice::Factor(2.0)),
                ..Default::default()
            },
            Some(from),
            None,
        )
        .immediate()
        .expect("get accounts success");

        transport.assert_request("eth_gasPrice", &[]);
        transport.assert_no_more_requests();

        assert_eq!(tx.from, from);
        assert_eq!(tx.gas_price, Some(U256::from(0x42 * 2)));
    }

    #[test]
    fn tx_build_local_with_explicit_gas_price() {
        let transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let from = addr!("0xffffffffffffffffffffffffffffffffffffffff");

        let tx = LocalBuildFuture::new(
            &web3,
            PartialTransaction {
                gas_price: Some(GasPrice::Value(1337.into())),
                ..Default::default()
            },
            Some(from),
            None,
        )
        .immediate()
        .expect("get accounts success");

        transport.assert_no_more_requests();

        assert_eq!(tx.from, from);
        assert_eq!(tx.gas_price, Some(1337.into()));
    }

    #[test]
    fn tx_build_locked() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let from = addr!("0x9876543210987654321098765432109876543210");
        let pw = "foobar";
        let to = addr!("0x0000000000000000000000000000000000000000");
        let signed = bytes!("0x0123456789"); // doesn't have to be valid, we don't check

        transport.add_response(json!({
            "raw": signed,
            "tx": {
                "hash": "0x0000000000000000000000000000000000000000000000000000000000000000",
                "nonce": "0x0",
                "from": from,
                "value": "0x0",
                "gas": "0x0",
                "gasPrice": "0x0",
                "input": "0x",
            }
        })); // sign transaction
        let tx = LockedBuildFuture::new(
            web3,
            PartialTransaction {
                to: Some(to),
                ..Default::default()
            },
            from,
            pw.into(),
            None,
        )
        .immediate()
        .expect("sign succeeded");

        transport.assert_request(
            "personal_signTransaction",
            &[
                json!({
                    "from": from,
                    "to": to,
                }),
                json!(pw),
            ],
        );
        transport.assert_no_more_requests();

        assert_eq!(tx, signed);
    }

    #[test]
    fn tx_build_locked_with_extra_gas_price() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let from = addr!("0x9876543210987654321098765432109876543210");
        let pw = "foobar";
        let gas_price = U256::from(1337);
        let to = addr!("0x0000000000000000000000000000000000000000");
        let signed = bytes!("0x0123456789"); // doesn't have to be valid, we don't check

        transport.add_response(json!(gas_price));
        transport.add_response(json!({
            "raw": signed,
            "tx": {
                "hash": "0x0000000000000000000000000000000000000000000000000000000000000000",
                "nonce": "0x0",
                "from": from,
                "value": "0x0",
                "gas": "0x0",
                "gasPrice": gas_price,
                "input": "0x",
            }
        })); // sign transaction
        let tx = LockedBuildFuture::new(
            web3,
            PartialTransaction {
                to: Some(to),
                gas_price: Some(GasPrice::Factor(2.0)),
                ..Default::default()
            },
            from,
            pw.into(),
            None,
        )
        .immediate()
        .expect("sign succeeded");

        transport.assert_request("eth_gasPrice", &[]);
        transport.assert_request(
            "personal_signTransaction",
            &[
                json!({
                    "from": from,
                    "to": to,
                    "gasPrice": gas_price * 2,
                }),
                json!(pw),
            ],
        );
        transport.assert_no_more_requests();

        assert_eq!(tx, signed);
    }

    #[test]
    fn tx_build_offline() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let key = key!("0x0102030405060708091011121314151617181920212223242526272829303132");
        let from: Address = key.public().address().into();
        let to = addr!("0x0000000000000000000000000000000000000000");

        let gas = uint!("0x9a5");
        let gas_price = uint!("0x1ce");
        let nonce = uint!("0x42");
        let chain_id = 77777;

        transport.add_response(json!(gas));
        transport.add_response(json!(gas_price));
        transport.add_response(json!(nonce));
        transport.add_response(json!(format!("{:#x}", chain_id)));

        let tx1 = TransactionBuilder::new(web3.clone())
            .from(Account::Offline(key.clone(), None))
            .to(to)
            .build()
            .immediate()
            .expect("sign succeeded")
            .raw()
            .expect("raw transaction");

        // assert that we ask the node for all the missing values
        transport.assert_request(
            "eth_estimateGas",
            &[json!({
                "from": from,
                "to": to,
            })],
        );
        transport.assert_request("eth_gasPrice", &[]);
        transport.assert_request("eth_getTransactionCount", &[json!(from), json!("latest")]);
        transport.assert_request("eth_chainId", &[]);
        transport.assert_no_more_requests();

        let tx2 = TransactionBuilder::new(web3)
            .from(Account::Offline(key, Some(chain_id)))
            .to(to)
            .gas(gas)
            .gas_price(gas_price.into())
            .nonce(nonce)
            .build()
            .immediate()
            .expect("sign succeeded")
            .raw()
            .expect("raw transaction");

        // assert that if we provide all the values then we can sign right away
        transport.assert_no_more_requests();

        // check that if we sign with same values we get same results
        assert_eq!(tx1, tx2);
    }
}
