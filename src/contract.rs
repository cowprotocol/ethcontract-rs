//! Abtraction for interacting with ethereum smart contracts. Provides methods
//! for sending transactions to contracts as well as querying current contract
//! state.

use crate::sign::TransactionData;
use crate::truffle::{Abi, Artifact};
use ethabi::{ErrorKind as AbiErrorKind, Function, Result as AbiResult};
use ethsign::{Error as EthsignError, SecretKey};
use futures::compat::Future01CompatExt;
use futures::future::{self, Either, FutureExt, TryFutureExt};
use std::future::Future;
use std::marker::PhantomData;
use std::num::ParseIntError;
use std::time::Duration;
use thiserror::Error;
use web3::api::Web3;
use web3::contract::tokens::{Detokenize, Tokenize};
use web3::contract::{Error as Web3ContractError, QueryResult};
use web3::error::Error as Web3Error;
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
    pub fn deployed(
        web3: Web3<T>,
        artifact: Artifact,
    ) -> impl Future<Output = Result<Instance<T>, DeployedError>> {
        web3.net().version().compat().map(|network_id| {
            let network_id = network_id?;
            let address = match artifact.networks.get(&network_id) {
                Some(network) => network.address,
                None => return Err(DeployedError::NotFound(network_id)),
            };

            // TODO(nlordell): validate that the contract @address is actually valid

            Ok(Instance {
                web3,
                abi: artifact.abi,
                address,
            })
        })
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

    /// Execute the call to the contract and retuen the data
    pub fn execute(self) -> impl Future<Output = Result<R, ExecutionError>> {
        QueryResult::new(
            self.web3.eth().call(
                CallRequest {
                    from: self.from,
                    to: self.address,
                    gas: None,
                    gas_price: None,
                    value: None,
                    data: Some(self.data),
                },
                self.block,
            ),
            self.function,
        )
        .compat()
        .map_err(ExecutionError::from)
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
    fn prepare(mut self) -> impl Future<Output = Result<Request, ExecutionError>> {
        use Either::*;

        let sign = match self.sign.take() {
            Some(s) => Left(future::ok(s)),
            None => Right(
                self.web3
                    .eth()
                    .accounts()
                    .compat()
                    .map_ok(|accounts| {
                        let account = accounts.get(0).cloned().unwrap_or_else(Address::zero);
                        Sign::Local(account, None)
                    })
                    .map_err(ExecutionError::from),
            ),
        };

        sign.and_then(move |sign| match sign {
            Sign::Local(from, condition) => {
                let tx = TransactionRequest {
                    from,
                    to: Some(self.address),
                    gas: self.gas,
                    gas_price: self.gas_price,
                    value: self.value,
                    data: Some(self.data),
                    nonce: self.nonce,
                    condition: condition,
                };
                Left(future::ok(Request::Tx(tx)))
            }
            Sign::Offline(key, chain_id) => {
                let from: Address = key.public().address().into();

                // for offline signing we need to finalize all transaction values
                // required for signing
                let gas = match self.gas.take() {
                    Some(g) => Left(future::ok(g)),
                    None => Right(
                        self.web3
                            .eth()
                            .estimate_gas(
                                CallRequest {
                                    from: Some(from),
                                    to: self.address,
                                    gas: None,
                                    gas_price: None,
                                    value: self.value,
                                    data: Some(self.data.clone()),
                                },
                                None,
                            )
                            .compat()
                            .map_err(ExecutionError::from),
                    ),
                };
                let gas_price = match self.gas_price.take() {
                    Some(p) => Left(future::ok(p)),
                    None => Right(
                        self.web3
                            .eth()
                            .gas_price()
                            .compat()
                            .map_err(ExecutionError::from),
                    ),
                };
                let nonce = match self.nonce.take() {
                    Some(n) => Left(future::ok(n)),
                    None => Right(
                        self.web3
                            .eth()
                            .transaction_count(from, None)
                            .compat()
                            .map_err(ExecutionError::from),
                    ),
                };

                // it looks like web3 defaults chain ID to network ID, although
                // this is not 'correct' in all cases it does work for most cases
                // like mainnet and various testnets and provides better safety
                // against replay attacks then just using no chain ID; so lets
                // reproduce that behaviour here
                let chain_id = match chain_id {
                    Some(id) => Left(future::ok(id)),
                    None => Right(
                        self.web3
                            .net()
                            .version()
                            .compat()
                            .map(|chain_id| Ok(chain_id?.parse()?)),
                    ),
                };

                Right(future::try_join4(gas, gas_price, nonce, chain_id).and_then(
                    move |(gas, gas_price, nonce, chain_id)| {
                        let tx = TransactionData {
                            nonce,
                            gas_price,
                            gas,
                            to: self.address,
                            value: self.value.unwrap_or_else(U256::zero),
                            data: &self.data,
                        };
                        let raw = match tx.sign(key, Some(chain_id)) {
                            Ok(r) => r,
                            Err(e) => return future::err(e.into()),
                        };
                        future::ok(Request::Raw(raw))
                    },
                ))
            }
        })
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
