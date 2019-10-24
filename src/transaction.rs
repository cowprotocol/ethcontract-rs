//! Implementation for setting up, signing, estimating gas and executing
//! transactions on the Ethereum network.

use crate::errors::ExecutionError;
use crate::future::{MaybeReady, CompatCallFuture, CompatSendTxWithConfirmation, Web3Unpin};
use crate::sign::TransactionData;
use ethsign::{Protected, SecretKey};
use futures::compat::Future01CompatExt;
use futures::future::{self, TryFuture, TryJoin4};
use futures::ready;
use std::future::Future;
use std::pin::Pin;
use std::str;
use std::task::{Context, Poll};
use std::time::Duration;
use web3::api::{Web3};
use web3::types::{
    Address, Bytes, CallRequest, RawTransaction, TransactionCondition, TransactionReceipt,
    TransactionRequest, H256, U256,
};
use web3::Transport;

/// Data used for building a transaction that modifies the blockchain. These
/// transactions can either be sent to be signed locally by the node or can be
/// signed offline.
#[derive(Clone, Debug)]
pub struct TransactionBuilder<T: Transport> {
    web3: Web3<T>,
    /// The sender of the transaction with the signing strategy to use. Defaults
    /// to locally signing on the node with the default acount.
    pub from: Option<Account>,
    /// The receiver of the transaction.
    pub to: Address,
    /// Optional gas amount to use for transaction. Defaults to estimated gas.
    pub gas: Option<U256>,
    /// Optional gas price to use for transaction. Defaults to estimated gas
    /// price.
    pub gas_price: Option<U256>,
    /// The ETH value to send with the transaction. Defaults to 0.
    pub value: Option<U256>,
    /// The data for the transaction. Defaults to empty data.
    pub data: Option<Bytes>,
    /// Optional nonce to use. Defaults to the signing account's current
    /// transaction count.
    pub nonce: Option<U256>,
}

/// The account type used for signing the transaction.
#[derive(Clone, Debug)]
pub enum Account {
    /// Let the node sign for a transaction with an unlocked account.
    Local(Address, Option<TransactionCondition>),
    /// Do online signing with a locked account with a password.
    Locked(Address, Protected, Option<TransactionCondition>),
    /// Do offline signing with private key and optionally specify chain ID.
    Offline(SecretKey, Option<u64>),
}

/// Represents a prepared and optionally signed transaction that is ready for
/// sending created by a `TransactionBuilder`.
pub enum Transaction {
    /// A structured transaction request to be signed locally by the node.
    Request(TransactionRequest),
    /// A signed raw transaction request.
    Raw(Bytes),
}

impl<T: Transport> TransactionBuilder<T> {
    /// Creates a new builder for a transaction.
    pub fn new(web3: Web3<T>) -> TransactionBuilder<T> {
        TransactionBuilder {
            web3,
            from: None,
            to: Address::zero(),
            gas: None,
            gas_price: None,
            value: None,
            data: None,
            nonce: None,
        }
    }

    /// Specify the signing method to use for the transaction, if not specified
    /// the the transaction will be locally signed with the default user.
    pub fn from(mut self, value: Account) -> TransactionBuilder<T> {
        self.from = Some(value);
        self
    }

    /// Specify the recepient of the transaction, if not specified the
    /// transaction will be sent to the 0 address (for deploying contracts).
    pub fn to(mut self, value: Address) -> TransactionBuilder<T> {
        self.to = value;
        self
    }

    /// Secify amount of gas to use, if not specified then a gas estimate will
    /// be used.
    pub fn gas(mut self, value: U256) -> TransactionBuilder<T> {
        self.gas = Some(value);
        self
    }

    /// Specify the gas price to use, if not specified then the estimated gas
    /// price will be used.
    pub fn gas_price(mut self, value: U256) -> TransactionBuilder<T> {
        self.gas_price = Some(value);
        self
    }

    /// Specify what how much ETH to transfer with the transaction, if not
    /// specified then no ETH will be sent.
    pub fn value(mut self, value: U256) -> TransactionBuilder<T> {
        self.value = Some(value);
        self
    }

    /// Specify the data to use for the transaction, if not specified, then empty
    /// data will be used.
    pub fn data(mut self, value: Bytes) -> TransactionBuilder<T> {
        self.data = Some(value);
        self
    }

    /// Specify the nonce for the transation, if not specified will use the
    /// current transaction count for the signing account.
    pub fn nonce(mut self, value: U256) -> TransactionBuilder<T> {
        self.nonce = Some(value);
        self
    }

    /// Estimate the gas required for this transaction.
    pub fn estimate_gas(self) -> EstimateGasFuture<T> {
        EstimateGasFuture::from_builder(self)
    }

    /// Build a prepared transaction that is ready to send.
    pub fn build(self) -> BuildFuture<T> {
        BuildFuture::from_builder(self)
    }

    /// Sign (if required) and execute the transaction. Returns the transaction
    /// hash that can be used to retrieve transaction information.
    pub fn execute(self) -> ExecuteFuture<T> {
        ExecuteFuture::from_builder(self)
    }

    /// Execute a transaction and wait for confirmation. Returns the transaction
    /// receipt for inspection.
    pub fn execute_and_confirm(
        self,
        poll_interval: Duration,
        confirmations: usize,
    ) -> ExecuteConfirmFuture<T> {
        ExecuteConfirmFuture::from_builder_with_confirm(self, poll_interval, confirmations)
    }
}

/// Future for estimating gas for a transaction.
pub struct EstimateGasFuture<T: Transport>(CompatCallFuture<T, U256>);

impl<T: Transport> EstimateGasFuture<T> {
    /// Create a instance from a `TransactionBuilder`.
    pub fn from_builder(builder: TransactionBuilder<T>) -> EstimateGasFuture<T> {
        let eth = builder.web3.eth();
        let from = builder.from.map(|from| match from {
            Account::Local(from, ..) => from,
            Account::Locked(from, ..) => from,
            Account::Offline(key, ..) => key.public().address().into(),
        });

        EstimateGasFuture(
            eth.estimate_gas(
                CallRequest {
                    from,
                    to: builder.to,
                    gas: None,
                    gas_price: None,
                    value: builder.value,
                    data: builder.data,
                },
                None,
            )
            .compat(),
        )
    }

    fn inner(self: Pin<&mut Self>) -> Pin<&mut CompatCallFuture<T, U256>> {
        Pin::new(&mut self.get_mut().0)
    }
}

impl<T: Transport> Future for EstimateGasFuture<T> {
    type Output = Result<U256, ExecutionError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.inner().poll(cx).map_err(ExecutionError::from)
    }
}

/// Future for preparing a transaction.
pub struct BuildFuture<T: Transport> {
    /// The internal build state for preparing the transaction.
    state: BuildState<T>,
}

impl<T: Transport> BuildFuture<T> {
    /// Create an instance from a `TransactionBuilder`.
    pub fn from_builder(builder: TransactionBuilder<T>) -> BuildFuture<T> {
        BuildFuture {
            state: BuildState::from_builder(builder),
        }
    }

    fn state(self: Pin<&mut Self>) -> &mut BuildState<T> {
        &mut self.get_mut().state
    }
}

impl<T: Transport> Future for BuildFuture<T> {
    type Output = Result<Transaction, ExecutionError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.state().poll_unpinned(cx)
    }
}

/// Internal build state for preparing transactions.
enum BuildState<T: Transport> {
    /// Waiting for list of accounts in order to determine from address so that
    /// we can return a `Request::Tx`.
    DefaultAccount {
        /// The transaction request being built.
        request: Option<TransactionRequest>,
        /// The inner future for retrieving the list of accounts on the node.
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
        params: TryJoin4<
            MaybeReady<CompatCallFuture<T, U256>>,
            MaybeReady<CompatCallFuture<T, U256>>,
            MaybeReady<CompatCallFuture<T, U256>>,
            MaybeReady<CompatCallFuture<T, String>>,
        >,
    },
}

impl<T: Transport> BuildState<T> {
    /// Create a `BuildState` from a `TransactionBuilder`
    fn from_builder(builder: TransactionBuilder<T>) -> BuildState<T> {
        match builder.from {
            None => BuildState::DefaultAccount {
                request: Some(TransactionRequest {
                    from: Address::zero(),
                    to: Some(builder.to),
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
                    to: Some(builder.to),
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
                    to: Some(builder.to),
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
                            None => MaybeReady::future($c.compat()),
                        }
                    };
                }

                let from = key.public().address().into();
                let eth = builder.web3.eth();
                let net = builder.web3.net();

                let gas = maybe!(
                    builder.gas,
                    eth.estimate_gas(
                        CallRequest {
                            from: Some(from),
                            to: builder.to,
                            gas: None,
                            gas_price: None,
                            value: builder.value,
                            data: builder.data.clone(),
                        },
                        None
                    )
                );

                let gas_price = maybe!(builder.gas_price, eth.gas_price());
                let nonce = maybe!(builder.nonce, eth.transaction_count(from, None));

                // it looks like web3 defaults chain ID to network ID, although
                // this is not 'correct' in all cases it does work for most cases
                // like mainnet and various testnets and provides better safety
                // against replay attacks then just using no chain ID; so lets
                // reproduce that behaviour here
                // TODO(nlordell): don't convert to and from string here
                let chain_id = maybe!(chain_id.map(|id| id.to_string()), net.version());

                BuildState::Offline {
                    key,
                    to: builder.to,
                    value: builder.value.unwrap_or_else(U256::zero),
                    data: builder.data.unwrap_or_else(Bytes::default),
                    params: future::try_join4(gas, gas_price, nonce, chain_id),
                }
            }
        }
    }

    fn poll_unpinned(&mut self, cx: &mut Context) -> Poll<Result<Transaction, ExecutionError>> {
        match self {
            BuildState::DefaultAccount { request, inner } => {
                Pin::new(inner).poll(cx).map(|accounts| {
                    let accounts = accounts?;

                    let mut request = request.take().expect("called once");
                    if let Some(from) = accounts.get(0) {
                        request.from = *from;
                    }

                    Ok(Transaction::Request(request))
                })
            }
            BuildState::Local { request } => Poll::Ready(Ok(Transaction::Request(
                request.take().expect("called once"),
            ))),
            BuildState::Locked { sign } => Pin::new(sign)
                .poll(cx)
                .map(|raw| Ok(Transaction::Raw(raw?.raw))),
            BuildState::Offline {
                key,
                to,
                value,
                data,
                params,
            } => Pin::new(params).poll(cx).map(|params| {
                let (gas, gas_price, nonce, chain_id) = params?;
                let chain_id = chain_id.parse()?;

                let tx = TransactionData {
                    nonce,
                    gas_price,
                    gas,
                    to: *to,
                    value: *value,
                    data: data,
                };
                let raw = tx.sign(key, Some(chain_id))?;

                Ok(Transaction::Raw(raw))
            }),
        }
    }
}

/// Future for optionally signing and then executing a transaction.
pub struct ExecuteFuture<T: Transport> {
    /// Internal execution state.
    state: ExecutionState<T, Web3Unpin<T>, CompatCallFuture<T, H256>>,
}

impl<T: Transport> ExecuteFuture<T> {
    /// Creates a new future from a `TransactionBuilder`
    pub fn from_builder(builder: TransactionBuilder<T>) -> ExecuteFuture<T> {
        let web3 = builder.web3.clone().into();
        let state = ExecutionState::from_builder_with_data(builder, web3);

        ExecuteFuture { state }
    }

    fn state(
        self: Pin<&mut Self>,
    ) -> &mut ExecutionState<T, Web3Unpin<T>, CompatCallFuture<T, H256>> {
        &mut self.get_mut().state
    }
}

impl<T: Transport> Future for ExecuteFuture<T> {
    type Output = Result<H256, ExecutionError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.state().poll_unpinned(cx, |web3, tx| match tx {
            Transaction::Request(tx) => web3.eth().send_transaction(tx).compat(),
            Transaction::Raw(tx) => web3.eth().send_raw_transaction(tx).compat(),
        })
    }
}

/// Future for optinally signing and then executing a transaction with
/// confirmation.
pub struct ExecuteConfirmFuture<T: Transport> {
    /// Internal execution state.
    state: ExecutionState<T, (Web3Unpin<T>, Duration, usize), CompatSendTxWithConfirmation<T>>,
}

impl<T: Transport> ExecuteConfirmFuture<T> {
    /// Creates a new future from a `TransactionBuilder`
    pub fn from_builder_with_confirm(
        builder: TransactionBuilder<T>,
        poll_interval: Duration,
        confirmations: usize,
    ) -> ExecuteConfirmFuture<T> {
        let web3 = builder.web3.clone().into();
        let state =
            ExecutionState::from_builder_with_data(builder, (web3, poll_interval, confirmations));

        ExecuteConfirmFuture { state }
    }

    fn state(
        self: Pin<&mut Self>,
    ) -> &mut ExecutionState<T, (Web3Unpin<T>, Duration, usize), CompatSendTxWithConfirmation<T>>
    {
        &mut self.get_mut().state
    }
}

impl<T: Transport> Future for ExecuteConfirmFuture<T> {
    type Output = Result<TransactionReceipt, ExecutionError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.as_mut()
            .state()
            .poll_unpinned(cx, |(web3, poll_interval, confirmations), tx| match tx {
                Transaction::Request(tx) => web3
                    .send_transaction_with_confirmation(tx, poll_interval, confirmations)
                    .compat(),
                Transaction::Raw(tx) => web3
                    .send_raw_transaction_with_confirmation(tx, poll_interval, confirmations)
                    .compat(),
            })
    }
}

/// Internal execution state for preparing and executing transactions.
enum ExecutionState<T, D, F>
where
    T: Transport,
    F: TryFuture + Unpin,
    F::Error: Into<ExecutionError>,
{
    /// Waiting for the transaction to be prepared to be sent.
    Build(BuildFuture<T>, Option<D>),
    /// Sending the request and waiting for the future to resolve.
    Send(F),
}

impl<T, D, F> ExecutionState<T, D, F>
where
    T: Transport,
    F: TryFuture + Unpin,
    F::Error: Into<ExecutionError>,
{
    /// Create a `ExecutionState` from a `TransactionBuilder`
    fn from_builder_with_data(builder: TransactionBuilder<T>, data: D) -> ExecutionState<T, D, F> {
        let build = BuildFuture::from_builder(builder);
        let data = Some(data);

        ExecutionState::Build(build, data)
    }

    /// Poll the state to drive the execution of its inner futures.
    fn poll_unpinned<S>(
        &mut self,
        cx: &mut Context,
        mut send_fn: S,
    ) -> Poll<Result<F::Ok, ExecutionError>>
    where
        S: FnMut(D, Transaction) -> F,
    {
        loop {
            match self {
                ExecutionState::Build(build, data) => {
                    let tx = ready!(Pin::new(build).poll(cx).map_err(ExecutionError::from));
                    let tx = match tx {
                        Ok(tx) => tx,
                        Err(err) => return Poll::Ready(Err(err)),
                    };

                    let data = data.take().expect("called once");
                    let send = send_fn(data, tx);
                    *self = ExecutionState::Send(send);
                }
                ExecutionState::Send(ref mut send) => {
                    return Pin::new(send)
                        .try_poll(cx)
                        .map_err(Into::<ExecutionError>::into)
                }
            }
        }
    }
}
