//! Implementation for setting up, signing, estimating gas and sending
//! transactions on the Ethereum network.

use crate::errors::ExecutionError;
use crate::future::{CompatCallFuture, CompatSendTxWithConfirmation, MaybeReady, Web3Unpin};
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
use web3::api::Web3;
use web3::types::{
    Address, Bytes, CallRequest, RawTransaction, TransactionCondition, TransactionReceipt,
    TransactionRequest, H256, U256,
};
use web3::Transport;

/// The account type used for signing the transaction.
#[derive(Clone, Debug)]
pub enum Account {
    /// Let the node sign for a transaction with an unlocked account.
    Local(Address, Option<TransactionCondition>),
    /// Do online signing with a locked account with a password.
    Locked(Address, Protected, Option<TransactionCondition>),
    /// Do offline signing with private key and optionally specify chain ID. If
    /// no chain ID is specified, then it will default to the network ID.
    Offline(SecretKey, Option<u64>),
}

impl Account {
    /// Returns the public address of an account.
    pub fn address(&self) -> Address {
        match self {
            Account::Local(address, _) => *address,
            Account::Locked(address, _, _) => *address,
            Account::Offline(key, _) => key.public().address().into(),
        }
    }
}

/// Represents a prepared and optionally signed transaction that is ready for
/// sending created by a `TransactionBuilder`.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Transaction {
    /// A structured transaction request to be signed locally by the node.
    Request(TransactionRequest),
    /// A signed raw transaction request.
    Raw(Bytes),
}

impl Transaction {
    /// Unwraps the transaction into a transaction request, returning None if the
    /// transaction is a raw transaction.
    pub fn request(self) -> Option<TransactionRequest> {
        match self {
            Transaction::Request(tx) => Some(tx),
            _ => None,
        }
    }

    /// Unwraps the transaction into its raw bytes, returning None if it is a
    /// transaction request.
    pub fn raw(self) -> Option<Bytes> {
        match self {
            Transaction::Raw(tx) => Some(tx),
            _ => None,
        }
    }
}

/// Data used for building a transaction that modifies the blockchain. These
/// transactions can either be sent to be signed locally by the node or can be
/// signed offline.
#[derive(Clone, Debug)]
#[must_use = "transactions do nothing unless you `.build()` or `.send()` them"]
pub struct TransactionBuilder<T: Transport> {
    web3: Web3<T>,
    /// The sender of the transaction with the signing strategy to use. Defaults
    /// to locally signing on the node with the default acount.
    pub from: Option<Account>,
    /// The receiver of the transaction.
    pub to: Option<Address>,
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

impl<T: Transport> TransactionBuilder<T> {
    /// Creates a new builder for a transaction.
    pub fn new(web3: Web3<T>) -> TransactionBuilder<T> {
        TransactionBuilder {
            web3,
            from: None,
            to: None,
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
        self.to = Some(value);
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

    /// Sign (if required) and send the transaction. Returns the transaction
    /// hash that can be used to retrieve transaction information.
    pub fn send(self) -> SendFuture<T> {
        SendFuture::from_builder(self)
    }

    /// Send a transaction and wait for confirmation. Returns the transaction
    /// receipt for inspection.
    pub fn send_and_confirm(
        self,
        poll_interval: Duration,
        confirmations: usize,
    ) -> SendAndConfirmFuture<T> {
        SendAndConfirmFuture::from_builder_with_confirm(self, poll_interval, confirmations)
    }
}

/// Future for estimating gas for a transaction.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct EstimateGasFuture<T: Transport>(CompatCallFuture<T, U256>);

impl<T: Transport> EstimateGasFuture<T> {
    /// Create a instance from a `TransactionBuilder`.
    pub fn from_builder(builder: TransactionBuilder<T>) -> EstimateGasFuture<T> {
        let eth = builder.web3.eth();
        let from = builder.from.map(|account| account.address());
        let to = builder.to.unwrap_or_else(Address::zero);

        EstimateGasFuture(
            eth.estimate_gas(
                CallRequest {
                    from,
                    to,
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
#[must_use = "futures do nothing unless you `.await` or poll them"]
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

/// Type alias for a call future that might already be resolved.
type MaybeCallFuture<T, R> = MaybeReady<CompatCallFuture<T, R>>;

/// Type alias for future retrieving the optional parameters that may not have
/// been specified by the transaction builder but are required for signing.
type ParamsFuture<T> = TryJoin4<
    MaybeCallFuture<T, U256>,
    MaybeCallFuture<T, U256>,
    MaybeCallFuture<T, U256>,
    MaybeCallFuture<T, String>,
>;

/// Internal build state for preparing transactions.
#[allow(clippy::large_enum_variant)]
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
        params: ParamsFuture<T>,
    },
}

impl<T: Transport> BuildState<T> {
    /// Create a `BuildState` from a `TransactionBuilder`
    fn from_builder(builder: TransactionBuilder<T>) -> BuildState<T> {
        match builder.from {
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
                            None => MaybeReady::future($c.compat()),
                        }
                    };
                }

                let from = key.public().address().into();
                let to = builder.to.unwrap_or_else(Address::zero);
                let eth = builder.web3.eth();
                let net = builder.web3.net();

                let gas = maybe!(
                    builder.gas,
                    eth.estimate_gas(
                        CallRequest {
                            from: Some(from),
                            to,
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
                    to,
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
                    data,
                };
                let raw = tx.sign(key, Some(chain_id))?;

                Ok(Transaction::Raw(raw))
            }),
        }
    }
}

/// Future for optionally signing and then sending a transaction.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct SendFuture<T: Transport> {
    /// Internal execution state.
    state: ExecutionState<T, Web3Unpin<T>, CompatCallFuture<T, H256>>,
}

impl<T: Transport> SendFuture<T> {
    /// Creates a new future from a `TransactionBuilder`
    pub fn from_builder(builder: TransactionBuilder<T>) -> SendFuture<T> {
        let web3 = builder.web3.clone().into();
        let state = ExecutionState::from_builder_with_data(builder, web3);

        SendFuture { state }
    }

    fn state(
        self: Pin<&mut Self>,
    ) -> &mut ExecutionState<T, Web3Unpin<T>, CompatCallFuture<T, H256>> {
        &mut self.get_mut().state
    }
}

impl<T: Transport> Future for SendFuture<T> {
    type Output = Result<H256, ExecutionError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.state().poll_unpinned(cx, |web3, tx| match tx {
            Transaction::Request(tx) => web3.eth().send_transaction(tx).compat(),
            Transaction::Raw(tx) => web3.eth().send_raw_transaction(tx).compat(),
        })
    }
}

/// Future for optinally signing and then sending a transaction with
/// confirmation.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct SendAndConfirmFuture<T: Transport> {
    /// Internal execution state.
    state: ExecutionState<T, (Web3Unpin<T>, Duration, usize), CompatSendTxWithConfirmation<T>>,
}

impl<T: Transport> SendAndConfirmFuture<T> {
    /// Creates a new future from a `TransactionBuilder`
    pub fn from_builder_with_confirm(
        builder: TransactionBuilder<T>,
        poll_interval: Duration,
        confirmations: usize,
    ) -> SendAndConfirmFuture<T> {
        let web3 = builder.web3.clone().into();
        let state =
            ExecutionState::from_builder_with_data(builder, (web3, poll_interval, confirmations));

        SendAndConfirmFuture { state }
    }

    fn state(
        self: Pin<&mut Self>,
    ) -> &mut ExecutionState<T, (Web3Unpin<T>, Duration, usize), CompatSendTxWithConfirmation<T>>
    {
        &mut self.get_mut().state
    }
}

impl<T: Transport> Future for SendAndConfirmFuture<T> {
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
    Building(BuildFuture<T>, Option<D>),
    /// Sending the request and waiting for the future to resolve.
    Sending(F),
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

        ExecutionState::Building(build, data)
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
                ExecutionState::Building(build, data) => {
                    let tx = ready!(Pin::new(build).poll(cx).map_err(ExecutionError::from));
                    let tx = match tx {
                        Ok(tx) => tx,
                        Err(err) => return Poll::Ready(Err(err)),
                    };

                    let data = data.take().expect("called once");
                    let send = send_fn(data, tx);
                    *self = ExecutionState::Sending(send);
                }
                ExecutionState::Sending(ref mut send) => {
                    return Pin::new(send)
                        .try_poll(cx)
                        .map_err(Into::<ExecutionError>::into)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::prelude::*;

    #[test]
    fn tx_builder_estimate_gas() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let to = addr!("0x0123456789012345678901234567890123456789");

        transport.add_response(json!("0x42")); // estimate gas response
        let estimate_gas = TransactionBuilder::new(web3)
            .to(to)
            .value(42.into())
            .estimate_gas();

        transport.assert_request(
            "eth_estimateGas",
            &[
                json!({
                    "to": to,
                    "value": "0x2a",
                }),
                json!("latest"), // block number
            ],
        );
        transport.assert_no_more_requests();

        let estimate_gas = estimate_gas.wait().expect("success");
        assert_eq!(estimate_gas, 0x42.into());
    }

    #[test]
    fn tx_send_local() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let from = addr!("0x9876543210987654321098765432109876543210");
        let to = addr!("0x0123456789012345678901234567890123456789");
        let hash = hash!("0x4242424242424242424242424242424242424242424242424242424242424242");

        transport.add_response(json!(hash)); // tansaction hash
        let tx = TransactionBuilder::new(web3)
            .from(Account::Local(from, Some(TransactionCondition::Block(100))))
            .to(to)
            .gas(1.into())
            .gas_price(2.into())
            .value(28.into())
            .data(Bytes(vec![0x13, 0x37]))
            .nonce(42.into())
            .send()
            .wait()
            .expect("transaction success");

        // assert that all the parameters are being used and that no extra
        // request was being sent (since no extra data from the node is needed)
        transport.assert_request(
            "eth_sendTransaction",
            &[json!({
                "from": from,
                "to": to,
                "gas": "0x1",
                "gasPrice": "0x2",
                "value": "0x1c",
                "data": "0x1337",
                "nonce": "0x2a",
                "condition": { "block": 100 },
            })],
        );
        transport.assert_no_more_requests();

        // assert the tx hash is what we expect it to be
        assert_eq!(tx, hash);
    }

    #[test]
    fn tx_build_default_account() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let accounts = [
            addr!("0x9876543210987654321098765432109876543210"),
            addr!("0x1111111111111111111111111111111111111111"),
            addr!("0x2222222222222222222222222222222222222222"),
        ];

        transport.add_response(json!(accounts)); // get accounts
        let tx = TransactionBuilder::new(web3)
            .build()
            .wait()
            .expect("get accounts success")
            .request()
            .expect("transaction request");

        transport.assert_request("eth_accounts", &[]);
        transport.assert_no_more_requests();

        // assert that if no from is specified that it uses the first account
        assert_eq!(tx.from, accounts[0]);
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
        let tx = TransactionBuilder::new(web3)
            .from(Account::Locked(from, pw.into(), None))
            .to(to)
            .build()
            .wait()
            .expect("sign succeeded")
            .raw()
            .expect("raw transaction");

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
        transport.add_response(json!(format!("{}", chain_id))); // chain id

        let tx1 = TransactionBuilder::new(web3.clone())
            .from(Account::Offline(key.clone(), None))
            .to(to)
            .build()
            .wait()
            .expect("sign succeeded")
            .raw()
            .expect("raw transaction");

        // assert that we ask the node for all the missing values
        transport.assert_request(
            "eth_estimateGas",
            &[
                json!({
                    "from": from,
                    "to": to,
                }),
                json!("latest"),
            ],
        );
        transport.assert_request("eth_gasPrice", &[]);
        transport.assert_request("eth_getTransactionCount", &[json!(from), json!("latest")]);
        transport.assert_request("net_version", &[]);
        transport.assert_no_more_requests();

        let tx2 = TransactionBuilder::new(web3)
            .from(Account::Offline(key, Some(chain_id)))
            .to(to)
            .gas(gas)
            .gas_price(gas_price)
            .nonce(nonce)
            .build()
            .wait()
            .expect("sign succeeded")
            .raw()
            .expect("raw transaction");

        // assert that if we provide all the values then we can sign right away
        transport.assert_no_more_requests();

        // check that if we sign with same values we get same results
        assert_eq!(tx1, tx2);
    }
}
