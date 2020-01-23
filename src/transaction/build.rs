//! This module implements transaction finalization from partial transaction
//! parameters. It provides futures for building `TransactionRequest` instances
//! and raw `Bytes` transactions from partial transaction parameters, where the
//! remaining parameters are queried from the node before finalizing the
//! transaction.

use crate::errors::ExecutionError;
use crate::future::{CompatCallFuture, MaybeReady};
use crate::sign::TransactionData;
use crate::transaction::gas_price::{
    GasPrice, ResolveGasPriceFuture, ResolveTransactionRequestGasPriceFuture,
};
use crate::transaction::{Account, EstimateGasFuture, Transaction, TransactionBuilder};
use ethsign::{Protected, SecretKey};
use futures::compat::Future01CompatExt;
use futures::future::{self, Join, TryJoin4};
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

/// Shared transaction options that are used when finalizing transactions into
/// either `TransactionRequest`s or raw signed transaction `Bytes`.
#[derive(Clone, Debug, Default)]
pub struct TransactionOptions {
    /// The receiver of the transaction.
    pub to: Option<Address>,
    /// The amount of gas to use for the transaction.
    pub gas: Option<U256>,
    /// The ETH value to send with the transaction.
    pub value: Option<U256>,
    /// The data for the transaction.
    pub data: Option<Bytes>,
    /// The transaction nonce.
    pub nonce: Option<U256>,
}

/// Transaction options specific to `TransactionRequests` since they may also
/// include a `TransactionCondition` that is not applicable to raw signed
/// transactions.
#[derive(Clone, Debug, Default)]
pub struct TransactionRequestOptions(pub TransactionOptions, pub Option<TransactionCondition>);

impl TransactionRequestOptions {
    /// Builds a `TransactionRequest` from a `TransactionRequestOptions` by
    /// specifying the missing parameters.
    fn build_request(self, from: Address, gas_price: Option<U256>) -> TransactionRequest {
        TransactionRequest {
            from,
            to: self.0.to,
            gas: self.0.gas,
            gas_price,
            value: self.0.value,
            data: self.0.data,
            nonce: self.0.nonce,
            condition: self.1,
        }
    }
}

/// Future for building a transaction so that it is ready to send. Can resolve
/// into either a `TransactionRequest` for sending locally signed transactions
/// or raw signed transaction `Bytes` when sending a raw transaction.
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[pin_project]
pub enum BuildFuture<T: Transport> {
    /// Locally signed transaction. Produces a `Transaction::Request` result.
    LocallySigned(#[pin] LocalBuildFuture<T>),
    /// Locally signed transaction with locked account. Produces a
    /// `Transaction::Raw` result.
    SignedWithLockedAccount(#[pin] LockedBuildFuture<T>),
    /// Offline signed transaction. Produces a `Transaction::Raw` result.
    OfflineSigned(#[pin] OfflineBuildFuture<T>),
}

impl<T: Transport> BuildFuture<T> {
    /// Create an instance from a `TransactionBuilder`.
    pub fn from_builder(builder: TransactionBuilder<T>) -> Self {
        let options = TransactionOptions {
            to: builder.to,
            gas: builder.gas,
            value: builder.value,
            data: builder.data,
            nonce: builder.nonce,
        };

        match builder.from {
            None => BuildFuture::LocallySigned(LocalBuildFuture::new(
                &builder.web3,
                None,
                builder.gas_price,
                TransactionRequestOptions(options, None),
            )),
            Some(Account::Local(from, condition)) => {
                BuildFuture::LocallySigned(LocalBuildFuture::new(
                    &builder.web3,
                    Some(from),
                    builder.gas_price,
                    TransactionRequestOptions(options, condition),
                ))
            }
            Some(Account::Locked(from, password, condition)) => {
                BuildFuture::SignedWithLockedAccount(LockedBuildFuture::new(
                    builder.web3,
                    from,
                    password,
                    builder.gas_price,
                    TransactionRequestOptions(options, condition),
                ))
            }
            Some(Account::Offline(key, chain_id)) => BuildFuture::OfflineSigned(
                OfflineBuildFuture::new(&builder.web3, key, chain_id, builder.gas_price, options),
            ),
        }
    }
}

impl<T: Transport> Future for BuildFuture<T> {
    type Output = Result<Transaction, ExecutionError>;

    #[project]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        #[project]
        match self.project() {
            BuildFuture::LocallySigned(local) => local
                .poll(cx)
                .map(|request| Ok(Transaction::Request(request?))),
            BuildFuture::SignedWithLockedAccount(locked) => {
                locked.poll(cx).map(|raw| Ok(Transaction::Raw(raw?)))
            }
            BuildFuture::OfflineSigned(offline) => {
                offline.poll(cx).map(|raw| Ok(Transaction::Raw(raw?)))
            }
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

/// Type alias for future retrieving default local account parameters.
type LocalParamsFuture<T> =
    Join<MaybeCallFuture<T, Vec<Address>>, ResolveTransactionRequestGasPriceFuture<T>>;

/// A future for building a locally signed transaction.
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[pin_project]
pub struct LocalBuildFuture<T: Transport> {
    /// The transaction options used for contructing a `TransactionRequest`. An
    /// `Option` is used here as the `Future` implementation requires moving the
    /// transaction options in order to construct the `TransactionRequest`.
    options: Option<TransactionRequestOptions>,
    /// The inner future for retrieving the list of accounts on the node and
    /// gas price estimation. These are the required missing transaction
    /// parameters that are needed to contruct the `TransactionRequest` from the
    /// transaction options.
    #[pin]
    params: LocalParamsFuture<T>,
}

impl<T: Transport> LocalBuildFuture<T> {
    /// Create a new future for building a locally singed transaction request
    /// from a partial transaction object and account information.
    pub fn new(
        web3: &Web3<T>,
        from: Option<Address>,
        gas_price: Option<GasPrice>,
        options: TransactionRequestOptions,
    ) -> Self {
        let options = Some(options);
        let params = {
            let eth = web3.eth();
            let accounts = maybe!(from.map(|from| vec![from]), eth.accounts().compat());
            let gas_price = GasPrice::resolve_for_transaction_request(gas_price, web3);
            future::join(accounts, gas_price)
        };

        LocalBuildFuture { params, options }
    }
}

impl<T: Transport> Future for LocalBuildFuture<T> {
    type Output = Result<TransactionRequest, ExecutionError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut this = self.project();
        this.params.as_mut().poll(cx).map(|(accounts, gas_price)| {
            let options = this.options.take().expect("future polled more than once");

            let from = accounts?.get(0).copied().unwrap_or_default();
            let gas_price = gas_price.transpose()?;

            Ok(options.build_request(from, gas_price))
        })
    }
}

/// A future for building a locally signed transaction with a locked account.
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[pin_project]
pub struct LockedBuildFuture<T: Transport> {
    /// The underlying web3 provider.
    web3: Web3<T>,
    /// The address of the locked account singing the transaction.
    from: Address,
    /// The password used for signing.
    password: Protected,
    /// The transaction options. Note that we use an `Option` here as the future
    /// needs to move the transaction options to contruct a `TransactionRequest`
    /// for signing.
    options: Option<TransactionRequestOptions>,
    /// The state of the build future.
    #[pin]
    state: LockedBuildState<T>,
}

/// The state of the `LockedBuildFuture`.
#[pin_project]
enum LockedBuildState<T: Transport> {
    /// Preparing the transaction request.
    ResolvingGasPrice(#[pin] ResolveTransactionRequestGasPriceFuture<T>),
    /// Signing the transaction request.
    Signing(#[pin] CompatCallFuture<T, RawTransaction>),
}

impl<T: Transport> LockedBuildFuture<T> {
    /// Create a new future for building a locally singed transaction request
    /// from a partial transaction object and account information.
    pub fn new(
        web3: Web3<T>,
        from: Address,
        password: Protected,
        gas_price: Option<GasPrice>,
        options: TransactionRequestOptions,
    ) -> Self {
        let options = Some(options);
        let gas_price = GasPrice::resolve_for_transaction_request(gas_price, &web3);
        LockedBuildFuture {
            web3,
            from,
            password,
            options,
            state: LockedBuildState::ResolvingGasPrice(gas_price),
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
                LockedBuildState::ResolvingGasPrice(gas_price) => {
                    let gas_price = match ready!(gas_price.poll(cx)).transpose() {
                        Ok(gas_price) => gas_price,
                        Err(err) => return Poll::Ready(Err(err.into())),
                    };

                    let options = this.options.take().expect("future called more than once");
                    let request = options.build_request(*this.from, gas_price);
                    let password = unsafe { str::from_utf8_unchecked(this.password.as_ref()) };

                    let sign = this
                        .web3
                        .personal()
                        .sign_transaction(request, password)
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
    ResolveGasPriceFuture<T>,
    MaybeCallFuture<T, U256>,
    MaybeCallFuture<T, U64>,
>;

/// A future for building a offline signed transaction.
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[pin_project]
pub struct OfflineBuildFuture<T: Transport> {
    /// The private key to use for signing.
    key: SecretKey,
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
        key: SecretKey,
        chain_id: Option<u64>,
        gas_price: Option<GasPrice>,
        options: TransactionOptions,
    ) -> Self {
        let to = options.to.unwrap_or_else(Address::zero);
        let value = options.value.unwrap_or_else(U256::zero);

        let params = {
            let from = key.public().address().into();
            let transport = web3.transport();
            let eth = web3.eth();

            let gas = maybe!(
                options.gas,
                EstimateGasFuture::from_request(
                    eth.clone(),
                    CallRequest {
                        from: Some(from),
                        to,
                        gas: None,
                        gas_price: None,
                        value: options.value,
                        data: options.data.clone(),
                    }
                )
                .into_inner()
            );
            let gas_price = gas_price.unwrap_or_default().resolve(web3);
            let nonce = maybe!(options.nonce, eth.transaction_count(from, None).compat());
            let chain_id = maybe!(
                chain_id.map(U64::from),
                CallFuture::new(transport.execute("eth_chainId", vec![])).compat()
            );

            future::try_join4(gas, gas_price, nonce, chain_id)
        };

        let data = options.data.unwrap_or_else(Bytes::default);

        OfflineBuildFuture {
            key,
            to,
            value,
            data,
            params,
        }
    }
}

impl<T: Transport> Future for OfflineBuildFuture<T> {
    type Output = Result<Bytes, ExecutionError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut this = self.project();
        this.params.as_mut().poll(cx).map(|params| {
            let (gas, gas_price, nonce, chain_id) = params?;
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
        let tx = LocalBuildFuture::new(&web3, None, None, TransactionRequestOptions::default())
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
            None,
            Some(GasPrice::Scaled(2.0)),
            TransactionRequestOptions::default(),
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
            Some(from),
            Some(GasPrice::Scaled(2.0)),
            TransactionRequestOptions::default(),
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
            Some(from),
            Some(GasPrice::Value(1337.into())),
            TransactionRequestOptions::default(),
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
            from,
            pw.into(),
            None,
            TransactionRequestOptions(
                TransactionOptions {
                    to: Some(to),
                    ..Default::default()
                },
                None,
            ),
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
            from,
            pw.into(),
            Some(GasPrice::Scaled(2.0)),
            TransactionRequestOptions::default(),
        )
        .immediate()
        .expect("sign succeeded");

        transport.assert_request("eth_gasPrice", &[]);
        transport.assert_request(
            "personal_signTransaction",
            &[
                json!({
                    "from": from,
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
        transport.add_response(json!(gas_price * 2));
        transport.add_response(json!(nonce));
        transport.add_response(json!(format!("{:#x}", chain_id)));

        let tx1 = OfflineBuildFuture::new(
            &web3,
            key.clone(),
            None,
            None,
            TransactionOptions {
                to: Some(to),
                ..Default::default()
            },
        )
        .immediate()
        .expect("sign succeeded");

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

        transport.add_response(json!(gas_price));

        let tx2 = OfflineBuildFuture::new(
            &web3,
            key.clone(),
            Some(chain_id),
            Some(GasPrice::Scaled(2.0)),
            TransactionOptions {
                to: Some(to),
                gas: Some(gas),
                nonce: Some(nonce),
                ..Default::default()
            },
        )
        .immediate()
        .expect("sign succeeded");

        transport.assert_request("eth_gasPrice", &[]);
        transport.assert_no_more_requests();

        let tx3 = OfflineBuildFuture::new(
            &web3,
            key,
            Some(chain_id),
            Some(GasPrice::Value(gas_price * 2)),
            TransactionOptions {
                to: Some(to),
                gas: Some(gas),
                nonce: Some(nonce),
                ..Default::default()
            },
        )
        .immediate()
        .expect("sign succeeded");

        // assert that if we provide all the values then we can sign right away
        transport.assert_no_more_requests();

        // check that if we sign with same values we get same results
        assert_eq!(tx1, tx3);
        assert_eq!(tx2, tx3);
    }
}
