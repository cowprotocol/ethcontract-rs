//! Abtraction for interacting with ethereum smart contracts. Provides methods
//! for sending transactions to contracts as well as querying current contract
//! state.

use crate::sign::TransactionData;
use crate::truffle::{Abi, Artifact};
use ethabi::{Function, Result as AbiResult};
use ethsign::{Error as EthsignError, SecretKey};
use futures::compat::Future01CompatExt;
use std::num::ParseIntError;
use thiserror::Error;
use web3::api::{Eth, Namespace, Net};
use web3::contract::tokens::{Detokenize, Tokenize};
use web3::contract::{Error as Web3ContractError, QueryResult};
use web3::error::Error as Web3Error;
use web3::types::{
    Address, BlockNumber, Bytes, CallRequest, TransactionCondition, TransactionRequest, H256, U256,
};
use web3::Transport;

/// Represents a contract instance at an address. Provides methods for
/// contract interaction.
pub struct Instance<T: Transport> {
    eth: Eth<T>,
    abi: Abi,
    address: Address,
}

impl<T: Transport> Instance<T> {
    /// Creates a new contract instance with the specified `web3` provider with
    /// the given `Abi` at the given `Address`.
    ///
    /// Note that this does not verify that a contract with a matchin `Abi` is
    /// actually deployed at the given address.
    pub fn at(eth: Eth<T>, abi: Abi, address: Address) -> Instance<T> {
        Instance { eth, abi, address }
    }

    /// Locates a deployed contract based on the current network ID reported by
    /// the `web3` provider from the given `Artifact`'s ABI and networks.
    ///
    /// Note that this does not verify that a contract with a matchin `Abi` is
    /// actually deployed at the given address.
    pub async fn deployed(eth: Eth<T>, artifact: Artifact) -> Result<Instance<T>, DeployedError> {
        let network_id = eth.net().version().compat().await?;
        let address = match artifact.networks.get(&network_id) {
            Some(network) => network.address,
            None => return Err(DeployedError::NotFound(network_id)),
        };

        // TODO(nlordell): validate that the contract @address is actually valid

        Ok(Instance {
            eth,
            abi: artifact.abi,
            address,
        })
    }

    /// Create a clone of the `web3::api::Eth` namespace using the current web3
    /// provider.
    fn eth(&self) -> Eth<T> {
        self.eth.clone()
    }

    /// Returns a call builder to setup a query to a smart contract that just
    /// gets evaluated on a node but does not actually commit anything to the
    /// block chain.
    pub fn call<S, P>(&self, name: S, params: P) -> AbiResult<CallBuilder<T>>
    where
        S: AsRef<str>,
        P: Tokenize,
    {
        let (function, data) = self.encode_abi(name, params)?;

        // take ownership here as it greatly simplifies dealing with futures
        // lifetime as it would require the contract Instance to live until
        // the end of the future
        let function = function.clone();

        Ok(CallBuilder {
            eth: self.eth(),
            function,
            address: self.address,
            data,
            from: None,
            block: None,
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
            eth: self.eth(),
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
pub struct CallBuilder<T: Transport> {
    eth: Eth<T>,
    function: Function,
    address: Address,
    data: Bytes,
    /// optional from address
    pub from: Option<Address>,
    /// optional block number
    pub block: Option<BlockNumber>,
}

impl<T: Transport> CallBuilder<T> {
    /// Specify from address for the contract call.
    pub fn from(mut self, address: Address) -> CallBuilder<T> {
        self.from = Some(address);
        self
    }

    /// Specify block number to use for the contract call.
    pub fn block(mut self, n: BlockNumber) -> CallBuilder<T> {
        self.block = Some(n);
        self
    }

    /// Execute the call to the contract and retuen the data
    pub async fn execute<R>(self) -> Result<R, ExecutionError>
    where
        R: Detokenize,
    {
        let result = QueryResult::new(
            self.eth.call(
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
        .await?;

        Ok(result)
    }
}

/// Data used for building a contract transaction that modifies the blockchain.
/// These transactions can either be sent to be signed locally by the node or can
/// be signed offline.
pub struct TransactionBuilder<T: Transport> {
    eth: Eth<T>,
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

    /// Sign (if required) and execute the transaction. Returns the transaction
    /// hash that can be used to retrieve transaction information.
    pub async fn execute(mut self) -> Result<H256, ExecutionError> {
        let sign = match self.sign.take() {
            Some(s) => s,
            None => {
                let accounts = self.eth.accounts().compat().await?;
                let account = accounts.get(0).cloned().unwrap_or_else(Address::zero);
                Sign::Local(account, None)
            }
        };

        let tx = match sign {
            Sign::Local(from, condition) => {
                self.eth
                    .send_transaction(TransactionRequest {
                        from,
                        to: Some(self.address),
                        gas: self.gas,
                        gas_price: self.gas_price,
                        value: self.value,
                        data: Some(self.data),
                        nonce: self.nonce,
                        condition: condition,
                    })
                    .compat()
                    .await?
            }
            Sign::Offline(key, chain_id) => {
                let from: Address = key.public().address().into();

                // for offline signing we need to finalize all transaction values
                // required for signing
                let gas = match self.gas.take() {
                    Some(g) => g,
                    None => {
                        self.eth
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
                            .await?
                    }
                };
                let gas_price = match self.gas_price.take() {
                    Some(p) => p,
                    None => self.eth.gas_price().compat().await?,
                };
                let nonce = match self.nonce.take() {
                    Some(n) => n,
                    None => self.eth.transaction_count(from, None).compat().await?,
                };

                // it looks like web3 defaults chain ID to network ID, although
                // this is not 'correct' in all cases it does work for most cases
                // like mainnet and various testnets and provides better safety
                // against replay attacks then just using no chain ID; so lets
                // reproduce that behaviour here
                let chain_id = match chain_id {
                    Some(id) => id,
                    None => self.eth.net().version().compat().await?.parse()?,
                };

                let tx = TransactionData {
                    nonce,
                    gas_price,
                    gas,
                    to: self.address,
                    value: self.value.unwrap_or_else(U256::zero),
                    data: &self.data,
                };
                let raw = tx.sign(key, Some(chain_id))?;

                self.eth.send_raw_transaction(raw).compat().await?
            }
        };

        Ok(tx)
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

/// Extension trait to add `net` sub-namespace to the `eth` namespace to more
/// closely match the `web3js` API.
trait EthExt<T: Transport> {
    fn net(&self) -> Net<T>;
}

impl<T: Transport> EthExt<T> for Eth<T> {
    fn net(&self) -> Net<T> {
        // do a little dance...
        Net::new(self.transport().clone())
    }
}
