//! This module implements transaction finalization from partial transaction
//! parameters. It provides futures for building `TransactionRequest` instances
//! and raw `Bytes` transactions from partial transaction parameters, where the
//! remaining parameters are queried from the node before finalizing the
//! transaction.

use crate::errors::ExecutionError;
use crate::future::{CompatCallFuture, MaybeReady};
use crate::sign::TransactionData;
use crate::transaction::estimate_gas::EstimateGasFuture;
use crate::transaction::{Account, Transaction, TransactionBuilder};
use ethsign::SecretKey;
use futures::compat::Future01CompatExt;
use futures::future::{self, TryJoin4};
use pin_project::{pin_project, project};
use std::future::Future;
use std::pin::Pin;
use std::str;
use std::task::{Context, Poll};
use web3::helpers::CallFuture;
use web3::types::{Address, Bytes, CallRequest, RawTransaction, TransactionRequest, U256, U64};
use web3::Transport;

/// Future for preparing a transaction.
#[must_use = "futures do nothing unless you `.await` or poll them"]
#[pin_project]
pub struct BuildFuture<T: Transport> {
    /// The internal build state for preparing the transaction.
    #[pin]
    state: BuildState<T>,
}

/// Type alias for a call future that might already be resolved.
type MaybeCallFuture<T, R> = MaybeReady<CompatCallFuture<T, R>>;

/// Type alias for future retrieving the optional parameters that may not have
/// been specified by the transaction builder but are required for signing.
type ParamsFuture<T> = TryJoin4<
    MaybeCallFuture<T, U256>,
    MaybeCallFuture<T, U256>,
    MaybeCallFuture<T, U256>,
    MaybeCallFuture<T, U64>,
>;

/// Internal build state for preparing transactions.
#[allow(clippy::large_enum_variant)]
#[pin_project]
enum BuildState<T: Transport> {
    /// Waiting for list of accounts in order to determine from address so that
    /// we can return a `Request::Tx`.
    DefaultAccount {
        /// The transaction request being built.
        request: Option<TransactionRequest>,
        /// The inner future for retrieving the list of accounts on the node.
        #[pin]
        inner: CompatCallFuture<T, Vec<Address>>,
    },

    /// Ready to resolve imediately to a `Transaction::Request` result.
    Local {
        /// The ready transaction request.
        request: Option<TransactionRequest>,
    },

    /// Waiting for the node to sign with a locked account.
    Locked {
        /// Future waiting for the node to sign the request with a locked account.
        #[pin]
        sign: CompatCallFuture<T, RawTransaction>,
    },

    /// Waiting for missing transaction parameters needed to sign and produce a
    /// `Request::Raw` result.
    Offline {
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
        params: ParamsFuture<T>,
    },
}

impl<T: Transport> BuildFuture<T> {
    /// Create an instance from a `TransactionBuilder`.
    pub fn from_builder(builder: TransactionBuilder<T>) -> Self {
        let state = match builder.from {
            None => BuildState::DefaultAccount {
                request: Some(TransactionRequest {
                    from: Address::zero(),
                    to: builder.to,
                    gas: builder.gas,
                    gas_price: builder.gas_price,
                    value: builder.value,
                    data: builder.data,
                    nonce: builder.nonce,
                    condition: None,
                }),
                inner: builder.web3.eth().accounts().compat(),
            },
            Some(Account::Local(from, condition)) => BuildState::Local {
                request: Some(TransactionRequest {
                    from,
                    to: builder.to,
                    gas: builder.gas,
                    gas_price: builder.gas_price,
                    value: builder.value,
                    data: builder.data,
                    nonce: builder.nonce,
                    condition,
                }),
            },
            Some(Account::Locked(from, password, condition)) => {
                let request = TransactionRequest {
                    from,
                    to: builder.to,
                    gas: builder.gas,
                    gas_price: builder.gas_price,
                    value: builder.value,
                    data: builder.data,
                    nonce: builder.nonce,
                    condition,
                };
                let password = unsafe { str::from_utf8_unchecked(password.as_ref()) };
                let sign = builder
                    .web3
                    .personal()
                    .sign_transaction(request, password)
                    .compat();

                BuildState::Locked { sign }
            }
            Some(Account::Offline(key, chain_id)) => {
                macro_rules! maybe {
                    ($o:expr, $c:expr) => {
                        match $o {
                            Some(v) => MaybeReady::ready(Ok(v)),
                            None => MaybeReady::future($c),
                        }
                    };
                }

                let from = key.public().address().into();
                let to = builder.to.unwrap_or_else(Address::zero);
                let eth = builder.web3.eth();
                let transport = builder.web3.transport();

                let gas = maybe!(
                    builder.gas,
                    EstimateGasFuture::from_request(
                        eth.clone(),
                        CallRequest {
                            from: Some(from),
                            to,
                            gas: None,
                            gas_price: None,
                            value: builder.value,
                            data: builder.data.clone(),
                        }
                    )
                    .into_inner()
                );

                let gas_price = maybe!(builder.gas_price, eth.gas_price().compat());
                let nonce = maybe!(builder.nonce, eth.transaction_count(from, None).compat());
                let chain_id = maybe!(
                    chain_id.map(U64::from),
                    CallFuture::new(transport.execute("eth_chainId", vec![])).compat()
                );

                BuildState::Offline {
                    key,
                    to,
                    value: builder.value.unwrap_or_else(U256::zero),
                    data: builder.data.unwrap_or_else(Bytes::default),
                    params: future::try_join4(gas, gas_price, nonce, chain_id),
                }
            }
        };

        BuildFuture { state }
    }
}

impl<T: Transport> Future for BuildFuture<T> {
    type Output = Result<Transaction, ExecutionError>;

    #[project]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        #[project]
        match self.project().state.project() {
            BuildState::DefaultAccount { request, inner } => inner.poll(cx).map(|accounts| {
                let accounts = accounts?;

                let mut request = request.take().expect("future polled more than once");
                if let Some(from) = accounts.get(0) {
                    request.from = *from;
                }

                Ok(Transaction::Request(request))
            }),
            BuildState::Local { request } => Poll::Ready(Ok(Transaction::Request(
                request.take().expect("future polled more than once"),
            ))),
            BuildState::Locked { sign } => sign.poll(cx).map(|raw| Ok(Transaction::Raw(raw?.raw))),
            BuildState::Offline {
                key,
                to,
                value,
                data,
                params,
            } => params.poll(cx).map(|params| {
                let (gas, gas_price, nonce, chain_id) = params?;
                let tx = TransactionData {
                    nonce,
                    gas_price,
                    gas,
                    to: *to,
                    value: *value,
                    data,
                };
                let raw = tx.sign(key, Some(chain_id.as_u64()))?;

                Ok(Transaction::Raw(raw))
            }),
        }
    }
}
