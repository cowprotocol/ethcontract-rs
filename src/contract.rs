//! Abtraction for interacting with ethereum smart contracts. Provides methods
//! for sending transactions to contracts as well as querying current contract
//! state.

use crate::future::MaybeReady;
use crate::sign::TransactionData;
use crate::truffle::{Abi, Artifact};
use ethabi::{ErrorKind as AbiErrorKind, Function, Result as AbiResult};
use ethsign::{Error as EthsignError, SecretKey};
use futures::compat::{Compat01As03, Future01CompatExt};
use futures::future::{self, TryFutureExt, TryJoin4};
use std::future::Future;
use std::marker::PhantomData;
use std::num::ParseIntError;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use thiserror::Error;
use web3::api::Web3;
use web3::contract::tokens::{Detokenize, Tokenize};
use web3::contract::{Error as Web3ContractError, QueryResult};
use web3::error::Error as Web3Error;
use web3::helpers::CallFuture;
use web3::types::{
    Address, BlockNumber, Bytes, CallRequest, TransactionCondition, TransactionReceipt,
    TransactionRequest, H256, U256,
};
use web3::Transport;

/// Represents a contract instance at an address. Provides methods for
/// contract interaction.
pub struct Instance<T: Transport> {
    web3: Web3<T>,
    abi: Abi,
    address: Address,
}

impl<T: Transport> Instance<T> {
    /// Creates a new contract instance with the specified `web3` provider with
    /// the given `Abi` at the given `Address`.
    ///
    /// Note that this does not verify that a contract with a matchin `Abi` is
    /// actually deployed at the given address.
    pub fn at(web3: Web3<T>, abi: Abi, address: Address) -> Instance<T> {
        Instance { web3, abi, address }
    }

    /// Locates a deployed contract based on the current network ID reported by
    /// the `web3` provider from the given `Artifact`'s ABI and networks.
    ///
    /// Note that this does not verify that a contract with a matchin `Abi` is
    /// actually deployed at the given address.
    pub fn deployed(web3: Web3<T>, artifact: Artifact) -> DeployedFuture<T> {
        DeployedFuture::from_args(web3, artifact)
    }

    /// Deploys a contract with the specified `web3` provider with the given
    /// `Artifact` byte code.
    pub fn deploy<P>(
        web3: Web3<T>,
        artifact: Artifact,
        params: P,
    ) -> AbiResult<TransactionBuilder<T>>
    where
        P: Tokenize,
    {
        // NOTE(nlordell): we can't just use `web3` implementation here as it
        //   does not support signing

        let tokens = params.into_tokens();
        let data = match artifact.abi.constructor {
            Some(constructor) => constructor.encode_input(artifact.bytecode.0, &tokens)?,
            None => {
                if tokens.len() != 0 {
                    // what `ethabi` returns when parameters don't match ABI
                    return Err(AbiErrorKind::InvalidData.into());
                }
                artifact.bytecode.0
            }
        };

        let _ = (data, web3);
        unimplemented!()
        // TODO(nlordell): we need to add confirmation + get contract address
        // Ok(TransactionBuilder {
        //     web3: self.web3(),
        //     address: self.address,
        //     data,
        //     gas: None,
        //     gas_price: None,
        //     value: None,
        //     nonce: None,
        //     sign: None,
        // })
    }

    /// Create a clone of the handle to our current `web3` provider.
    fn web3(&self) -> Web3<T> {
        self.web3.clone()
    }

    /// Returns the contract address being used by this instance.
    pub fn address(&self) -> Address {
        self.address
    }

    /// Returns a call builder to setup a query to a smart contract that just
    /// gets evaluated on a node but does not actually commit anything to the
    /// block chain.
    pub fn call<S, P, R>(&self, name: S, params: P) -> AbiResult<CallBuilder<T, R>>
    where
        S: AsRef<str>,
        P: Tokenize,
        R: Detokenize,
    {
        let (function, data) = self.encode_abi(name, params)?;

        // take ownership here as it greatly simplifies dealing with futures
        // lifetime as it would require the contract Instance to live until
        // the end of the future
        let function = function.clone();

        Ok(CallBuilder {
            web3: self.web3(),
            function,
            address: self.address,
            data,
            from: None,
            block: None,
            _result: PhantomData,
        })
    }

    /// Returns a transaction builder to setup a transaction
    pub fn send<S, P>(&self, name: S, params: P) -> AbiResult<TransactionBuilder<T>>
    where
        S: AsRef<str>,
        P: Tokenize,
    {
        let (_, data) = self.encode_abi(name, params)?;
        Ok(TransactionBuilder {
            web3: self.web3(),
            address: self.address,
            data,
            gas: None,
            gas_price: None,
            value: None,
            nonce: None,
            sign: None,
        })
    }

    /// Utility function to locate a function by name and encode the function
    /// signature and parameters into data bytes to be sent to a contract.
    #[inline(always)]
    fn encode_abi<S, P>(&self, name: S, params: P) -> AbiResult<(&Function, Bytes)>
    where
        S: AsRef<str>,
        P: Tokenize,
    {
        let function = self.abi.function(name.as_ref())?;
        let data = function.encode_input(&params.into_tokens())?;

        Ok((function, data.into()))
    }
}

/// Type alias for Compat01As03<CallFuture<...>> since it is used a lot.
type CompatCallFuture<T, R> = Compat01As03<CallFuture<R, <T as Transport>::Out>>;

/// Helper type for wrapping `Web3` as `Unpin`.
struct Web3Unpin<T: Transport>(Web3<T>);

impl<T: Transport> Into<Web3<T>> for Web3Unpin<T> {
    fn into(self) -> Web3<T> {
        self.0
    }
}

impl<T: Transport> From<Web3<T>> for Web3Unpin<T> {
    fn from(web3: Web3<T>) -> Self {
        Web3Unpin(web3)
    }
}

// It is safe to mark this type as `Unpin` since `Web3<T>` *should be* `Unpin`
// even if T is not.
// TODO(nlordell): verify this is safe
impl<T: Transport> Unpin for Web3Unpin<T> {}

/// Future for creating a deployed contract instance.
pub struct DeployedFuture<T: Transport> {
    /// Deployed arguments: `web3` provider and artifact.
    args: Option<(Web3Unpin<T>, Artifact)>,
    /// Underlying future for retrieving the network ID.
    network_id: CompatCallFuture<T, String>,
}

impl<T: Transport> DeployedFuture<T> {
    fn from_args(web3: Web3<T>, artifact: Artifact) -> DeployedFuture<T> {
        let net = web3.net();
        DeployedFuture {
            args: Some((web3.into(), artifact)),
            network_id: net.version().compat(),
        }
    }

    /// Take value of our passed in `web3` provider.
    fn args(self: Pin<&mut Self>) -> (Web3<T>, Artifact) {
        let (web3, artifact) = self
            .get_mut()
            .args
            .take()
            .expect("should be called only once");
        (web3.into(), artifact)
    }

    /// Get a pinned reference to the inner `CallFuture` for retrieving the
    /// current network ID.
    fn network_id(self: Pin<&mut Self>) -> Pin<&mut CompatCallFuture<T, String>> {
        Pin::new(&mut self.get_mut().network_id)
    }
}

impl<T: Transport> Future for DeployedFuture<T> {
    type Output = Result<Instance<T>, DeployedError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.as_mut().network_id().poll(cx).map(|network_id| {
            let network_id = network_id?;
            let (web3, artifact) = self.args();

            let address = match artifact.networks.get(&network_id) {
                Some(network) => network.address,
                None => return Err(DeployedError::NotFound(network_id)),
            };

            Ok(Instance {
                web3,
                abi: artifact.abi,
                address,
            })
        })
    }
}

/// Error that can occur while locating a deployed contract.
#[derive(Debug, Error)]
pub enum DeployedError {
    /// An error occured while performing a web3 call.
    #[error("web3 error: {0}")]
    Web3(#[from] Web3Error),

    /// No previously deployed contract could be found on the network being used
    /// by the current `web3` provider.
    #[error("could not find deployed contract for network {0}")]
    NotFound(String),
}

/// Data used for building a contract call (i.e. query). Contract calls do not
/// modify the blockchain and as such do not require gas, signing and cannot
/// accept value.
#[derive(Clone, Debug)]
pub struct CallBuilder<T: Transport, R: Detokenize> {
    web3: Web3<T>,
    function: Function,
    address: Address,
    data: Bytes,
    /// optional from address
    pub from: Option<Address>,
    /// optional block number
    pub block: Option<BlockNumber>,
    _result: PhantomData<R>,
}

impl<T: Transport, R: Detokenize> CallBuilder<T, R> {
    /// Specify from address for the contract call.
    pub fn from(mut self, address: Address) -> CallBuilder<T, R> {
        self.from = Some(address);
        self
    }

    /// Specify block number to use for the contract call.
    pub fn block(mut self, n: BlockNumber) -> CallBuilder<T, R> {
        self.block = Some(n);
        self
    }

    /// Execute the call to the contract and return the data
    pub fn execute(self) -> ExecuteCallFuture<T, R> {
        ExecuteCallFuture::from_builder(self)
    }
}

/// Future representing a pending contract call (i.e. query) to be resolved when
/// the call completes.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct ExecuteCallFuture<T: Transport, R: Detokenize>(Compat01As03<QueryResult<R, T::Out>>);

impl<T: Transport, R: Detokenize> ExecuteCallFuture<T, R> {
    /// Construct a new `ExecuteCallFuture` from a `CallBuilder`.
    fn from_builder(builder: CallBuilder<T, R>) -> ExecuteCallFuture<T, R> {
        ExecuteCallFuture(
            QueryResult::new(
                builder.web3.eth().call(
                    CallRequest {
                        from: builder.from,
                        to: builder.address,
                        gas: None,
                        gas_price: None,
                        value: None,
                        data: Some(builder.data),
                    },
                    builder.block,
                ),
                builder.function,
            )
            .compat(),
        )
    }

    /// Get a pinned reference to the inner `QueryResult` web3 future taht is
    /// actually driving the query.
    fn inner(self: Pin<&mut Self>) -> Pin<&mut Compat01As03<QueryResult<R, T::Out>>> {
        Pin::new(&mut self.get_mut().0)
    }
}

impl<T: Transport, R: Detokenize> Future for ExecuteCallFuture<T, R> {
    type Output = Result<R, ExecutionError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.inner()
            .poll(cx)
            .map(|result| result.map_err(ExecutionError::from))
    }
}

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

/// Error that can occur while executing a contract call or transaction.
#[derive(Debug, Error)]
pub enum ExecutionError {
    /// An error occured while performing a web3 call.
    #[error("web3 error: {0}")]
    Web3(#[from] Web3Error),

    /// An error occured while performing a web3 contract call.
    #[error("web3 contract error: {0}")]
    Web3Contract(#[from] Web3ContractError),

    /// An error occured while parsing numbers received from Web3 calls.
    #[error("parse error: {0}")]
    Parse(#[from] ParseIntError),

    /// An error occured while signing a transaction offline.
    #[error("offline sign error: {0}")]
    Sign(#[from] EthsignError),
}
