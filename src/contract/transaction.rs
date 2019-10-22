use crate::contract::errors::ExecutionError;
use crate::contract::util::CompatCallFuture;
use crate::future::MaybeReady;
use crate::sign::TransactionData;
use ethsign::SecretKey;
use futures::future::{self, TryJoin4, TryFutureExt};
use futures::compat::Future01CompatExt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use web3::api::Web3;
use web3::types::{
    Address, Bytes, CallRequest, TransactionCondition, TransactionReceipt, TransactionRequest,
    H256, U256,
};
use web3::Transport;

/// Data used for building a contract transaction that modifies the blockchain.
/// These transactions can either be sent to be signed locally by the node or can
/// be signed offline.
#[derive(Clone, Debug)]
pub struct TransactionBuilder<T: Transport> {
    web3: Web3<T>,
    address: Address,
    data: Bytes,
    /// The signing strategy to use. Defaults to locally signing on the node with
    /// the default acount.
    pub sign: Option<Sign>,
    /// Optional gas amount to use for transaction. Defaults to estimated gas.
    pub gas: Option<U256>,
    /// Optional gas price to use for transaction. Defaults to estimated gas
    /// price.
    pub gas_price: Option<U256>,
    /// The ETH value to send with the transaction. Defaults to 0.
    pub value: Option<U256>,
    /// Optional nonce to use. Defaults to the signing account's current
    /// transaction count.
    pub nonce: Option<U256>,
}

/// How the transaction should be signed
#[derive(Clone, Debug)]
pub enum Sign {
    /// Let the node locally sign for address
    Local(Address, Option<TransactionCondition>),
    /// Do offline signing with private key and optionally specify chain ID
    Offline(SecretKey, Option<u64>),
}

/// Represents either a structured or raw transaction request.
enum Request {
    /// A structured transaction request to be signed locally by the node.
    Tx(TransactionRequest),
    /// A signed raw transaction request.
    Raw(Bytes),
}

impl<T: Transport> TransactionBuilder<T> {
    /// Creates a new builder for a contract transaction.
    pub fn new(web3: Web3<T>, address: Address, data: Bytes) -> TransactionBuilder<T> {
        TransactionBuilder {
            web3,
            address,
            data,
            gas: None,
            gas_price: None,
            value: None,
            nonce: None,
            sign: None,
        }
    }
    
    /// Specify the signing method to use for the transaction, if not specified
    /// the the transaction will be locally signed with the default user.
    pub fn sign(mut self, value: Sign) -> TransactionBuilder<T> {
        self.sign = Some(value);
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

    /// Specify the nonce for the transation, if not specified will use the
    /// current transaction count for the signing account.
    pub fn nonce(mut self, value: U256) -> TransactionBuilder<T> {
        self.nonce = Some(value);
        self
    }

    /// Prepares a transaction for execution.
    fn prepare(self) -> PrepareTransactionFuture<T> {
        PrepareTransactionFuture::from_builder(self)
    }

    /// Sign (if required) and execute the transaction. Returns the transaction
    /// hash that can be used to retrieve transaction information.
    pub fn execute(self) -> impl Future<Output = Result<H256, ExecutionError>> {
        let eth = self.web3.eth();
        self.prepare().and_then(move |request| {
            let send = match request {
                Request::Tx(tx) => eth.send_transaction(tx),
                Request::Raw(tx) => eth.send_raw_transaction(tx),
            };
            send.compat().map_err(ExecutionError::from)
        })
    }

    /// Execute a transaction and wait for confirmation. Returns the transaction
    /// receipt for inspection.
    pub fn execute_and_confirm(
        self,
        poll_interval: Duration,
        confirmations: usize,
    ) -> impl Future<Output = Result<TransactionReceipt, ExecutionError>> {
        let web3 = self.web3.clone();
        self.prepare().and_then(move |request| {
            let send = match request {
                Request::Tx(tx) => {
                    web3.send_transaction_with_confirmation(tx, poll_interval, confirmations)
                }
                Request::Raw(tx) => {
                    web3.send_raw_transaction_with_confirmation(tx, poll_interval, confirmations)
                }
            };
            send.compat().map_err(ExecutionError::from)
        })
    }
}

/// Internal future for preparing a transaction for sending.
enum PrepareTransactionFuture<T: Transport> {
    /// Waiting for list of accounts in order to determine from address so that
    /// we can return a `Request::Tx`.
    TxDefaultAccount {
        /// The transaction request being built.
        request: Option<TransactionRequest>,

        /// The inner future for retrieving the list of accounts on the node.
        inner: CompatCallFuture<T, Vec<Address>>,
    },

    /// Ready to produce a `Request::Tx` result.
    Tx {
        /// The ready transaction request.
        request: Option<TransactionRequest>,
    },

    /// Waiting for missing transaction parameters needed to sign and produce a
    /// `Request::Raw` result.
    Raw {
        /// The private key to use for signing.
        key: SecretKey,

        /// The contract address.
        address: Address,

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

impl<T: Transport> PrepareTransactionFuture<T> {
    /// Create a `PrepareTransactionFuture` from a `PrepareTransactionBuilder`
    fn from_builder(builder: TransactionBuilder<T>) -> PrepareTransactionFuture<T> {
        match builder.sign {
            None => PrepareTransactionFuture::TxDefaultAccount {
                request: Some(TransactionRequest {
                    from: Address::zero(),
                    to: Some(builder.address),
                    gas: builder.gas,
                    gas_price: builder.gas_price,
                    value: builder.value,
                    data: Some(builder.data),
                    nonce: builder.nonce,
                    condition: None,
                }),
                inner: builder.web3.eth().accounts().compat(),
            },
            Some(Sign::Local(from, condition)) => PrepareTransactionFuture::Tx {
                request: Some(TransactionRequest {
                    from,
                    to: Some(builder.address),
                    gas: builder.gas,
                    gas_price: builder.gas_price,
                    value: builder.value,
                    data: Some(builder.data),
                    nonce: builder.nonce,
                    condition,
                }),
            },
            Some(Sign::Offline(key, chain_id)) => {
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
                            to: builder.address,
                            gas: None,
                            gas_price: None,
                            value: builder.value,
                            data: Some(builder.data.clone()),
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

                PrepareTransactionFuture::Raw {
                    key,
                    address: builder.address,
                    value: builder.value.unwrap_or_else(U256::zero),
                    data: builder.data,
                    params: future::try_join4(gas, gas_price, nonce, chain_id),
                }
            }
        }
    }
}

impl<T: Transport> Future for PrepareTransactionFuture<T> {
    type Output = Result<Request, ExecutionError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let unpinned = self.get_mut();
        match unpinned {
            PrepareTransactionFuture::TxDefaultAccount { request, inner } => {
                Pin::new(inner).poll(cx).map(|accounts| {
                    let accounts = accounts?;
                    let mut request = request.take().expect("should be called only once");

                    if let Some(from) = accounts.get(0) {
                        request.from = *from;
                    }

                    Ok(Request::Tx(request))
                })
            }
            PrepareTransactionFuture::Tx { request } => {
                let request = request.take().expect("should be called only once");
                Poll::Ready(Ok(Request::Tx(request)))
            }
            PrepareTransactionFuture::Raw {
                key,
                address,
                value,
                data,
                params,
            } => Pin::new(params).poll(cx).map(|result| {
                let (gas, gas_price, nonce, chain_id) = result?;
                let chain_id = chain_id.parse()?;

                let tx = TransactionData {
                    nonce,
                    gas_price,
                    gas,
                    to: *address,
                    value: *value,
                    data: data,
                };
                let raw = tx.sign(key, Some(chain_id))?;

                Ok(Request::Raw(raw))
            }),
        }
    }
}
