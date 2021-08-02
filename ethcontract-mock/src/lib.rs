#![deny(missing_docs, unsafe_code)]

//! This crate allows emulating ethereum node with a limited number
//! of supported RPC calls, enabling you to mock ethereum contracts.

use crate::predicate::TuplePredicate;
use crate::range::TimesRange;
use ethcontract::common::hash::H32;
use ethcontract::common::Abi;
use ethcontract::dyns::{DynInstance, DynTransport, DynWeb3};
use ethcontract::tokens::Tokenize;
use ethcontract::{Address, U256};
use std::marker::PhantomData;

#[doc(no_inline)]
pub use ethcontract::contract::Signature;

mod details;
mod predicate;
mod range;

/// Mock ethereum node.
#[derive(Clone)]
pub struct Mock {
    transport: details::MockTransport,
}

impl Mock {
    /// Creates a new mock chain.
    pub fn new(chain_id: u64) -> Self {
        Mock {
            transport: details::MockTransport::new(chain_id),
        }
    }

    /// Creates a `Web3` object that can be used to interact with
    /// the mocked chain.
    pub fn web3(&self) -> DynWeb3 {
        DynWeb3::new(self.transport())
    }

    /// Creates a `Transport` object that can be used to interact with
    /// the mocked chain.
    pub fn transport(&self) -> DynTransport {
        DynTransport::new(self.transport.clone())
    }

    /// Deploys a new mocked contract and returns an object that allows
    /// configuring expectations for contract methods.
    pub fn deploy(&self, abi: Abi) -> Contract {
        let address = self.transport.deploy(&abi);
        Contract {
            transport: self.transport.clone(),
            address,
            abi,
        }
    }

    /// Updates gas price that is returned by RPC call `eth_gasPrice`.
    ///
    /// Mock node does not simulate gas consumption, so this value does not
    /// affect anything if you don't call `eth_gasPrice`.
    pub fn update_gas_price(&self, gas_price: u64) {
        self.transport.update_gas_price(gas_price);
    }

    /// Verifies that all expectations on all contracts have been met,
    /// then clears all expectations.
    pub fn checkpoint(&self) {
        todo!()
    }
}

impl std::fmt::Debug for Mock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Mock")
    }
}

/// A mocked contract deployed by the mock node.
///
/// This struct allows setting up expectations on which contract methods
/// will be called, with what arguments, in what order, etc.
pub struct Contract {
    transport: details::MockTransport,
    address: Address,
    abi: Abi,
}

impl Contract {
    /// Creates a `Web3` object that can be used to interact with
    /// the mocked chain on which this contract is deployed.
    pub fn web3(&self) -> DynWeb3 {
        DynWeb3::new(self.transport())
    }

    /// Creates a `Transport` object that can be used to interact with
    /// the mocked chain.
    pub fn transport(&self) -> DynTransport {
        DynTransport::new(self.transport.clone())
    }

    /// Creates a contract `Instance` that can be used to interact with
    /// this contract.
    pub fn instance(&self) -> DynInstance {
        DynInstance::at(self.web3(), self.abi.clone(), self.address)
    }

    /// Consumes this object and transforms it into a contract `Instance`
    /// that can be used to interact with this contract.
    pub fn into_instance(self) -> DynInstance {
        DynInstance::at(self.web3(), self.abi, self.address)
    }

    /// Returns a reference to the contract's ABI.
    pub fn abi(&self) -> &Abi {
        &self.abi
    }

    /// Returns contract's address.
    pub fn address(&self) -> Address {
        self.address
    }

    /// Adds a new expectation for contract method. See [`Expectation`].
    pub fn expect<P: Tokenize + Send + 'static, R: Tokenize + Send + 'static>(
        &self,
        signature: impl Into<Signature<P, R>>,
    ) -> Expectation<P, R> {
        todo!()
    }

    /// Adds a new expectation for contract method that only matches view calls.
    ///
    /// This is an equivalent of [`expect`] followed by [`allow_transactions`]`(false)`.
    ///
    /// [`expect`]: Contract::expect
    /// [`allow_transactions`]: Expectation::allow_transactions
    pub fn expect_call<P: Tokenize + Send + 'static, R: Tokenize + Send + 'static>(
        &self,
        signature: impl Into<Signature<P, R>>,
    ) -> Expectation<P, R> {
        self.expect(signature).allow_transactions(false)
    }

    /// Adds a new expectation for contract method that only matches transactions.
    ///
    /// This is an equivalent of [`expect`] followed by [`allow_calls`]`(false)`.
    ///
    /// [`expect`]: Contract::expect
    /// [`allow_calls`]: Expectation::allow_calls
    pub fn expect_transaction<P: Tokenize + Send + 'static, R: Tokenize + Send + 'static>(
        &self,
        signature: impl Into<Signature<P, R>>,
    ) -> Expectation<P, R> {
        self.expect(signature).allow_calls(false)
    }

    /// Verifies that all expectations on this contract have been met,
    /// then clears all expectations.
    pub fn checkpoint(&self) {
        todo!()
    }
}

/// Expectation for contract method.
pub struct Expectation<P: Tokenize + Send + 'static, R: Tokenize + Send + 'static> {
    _ph: PhantomData<(P, R)>,
}

impl<P: Tokenize + Send + 'static, R: Tokenize + Send + 'static> Expectation<P, R> {
    /// Specifies how many times this expectation can be called.
    pub fn times(self, times: impl Into<TimesRange>) -> Self {
        todo!()
    }

    /// Indicates that this expectation can be called exactly zero times.
    ///
    /// See [`times`] for more info.
    ///
    /// [`times`]: Expectation::times
    pub fn never(self) -> Self {
        self.times(0)
    }

    /// Indicates that this expectation can be called exactly one time.
    ///
    /// See [`times`] for more info.
    ///
    /// [`times`]: Expectation::times
    pub fn once(self) -> Self {
        self.times(1)
    }

    /// Adds this expectation to a sequence.
    pub fn in_sequence(self, sequence: &mut mockall::Sequence) -> Self {
        todo!()
    }

    /// Sets number of blocks that should be mined on top of the transaction
    /// block. This method can be useful when there are custom transaction
    /// confirmation settings.
    pub fn confirmations(self, confirmations: u64) -> Self {
        todo!()
    }

    /// Sets predicate for this expectation.
    pub fn predicate<T>(self, pred: T) -> Self
    where
        T: TuplePredicate<P> + Send + 'static,
        <T as predicate::TuplePredicate<P>>::P: Send,
    {
        todo!()
    }

    /// Sets predicate function for this expectation. This function accepts
    /// a tuple of method's arguments and returns `true` if this
    /// expectation should be called. See [`predicate`] for more info.
    ///
    /// This method will overwrite any predicate that was set before.
    ///
    /// [`predicate`]: Expectation::predicate
    pub fn predicate_fn(self, pred: impl Fn(&P) -> bool + Send + 'static) -> Self {
        todo!()
    }

    /// Sets predicate function for this expectation. This function accepts
    /// a [call context] and a tuple of method's arguments and returns `true`
    /// if this expectation should be called. See [`predicate`] for more info.
    ///
    /// This method will overwrite any predicate that was set before.
    ///
    /// [call context]: CallContext
    /// [`predicate`]: Expectation::predicate
    pub fn predicate_fn_ctx(
        self,
        pred: impl Fn(&CallContext, &P) -> bool + Send + 'static,
    ) -> Self {
        todo!()
    }

    /// Indicates that this expectation only applies to view calls.
    ///
    /// This method will not override predicates set by [`predicate`] and
    /// similar methods.
    ///
    /// See also [`Contract::expect_call`].
    ///
    /// [`predicate`]: Expectation::predicate
    pub fn allow_calls(self, allow_calls: bool) -> Self {
        todo!()
    }

    /// Indicates that this expectation only applies to transactions.
    ///
    /// This method will not override predicates set by [`predicate`] and
    /// similar methods.
    ///
    /// See also [`Contract::expect_transaction`].
    ///
    /// [`predicate`]: Expectation::predicate
    pub fn allow_transactions(self, allow_transactions: bool) -> Self {
        todo!()
    }

    /// Sets return value of the method.
    ///
    /// By default, call to this expectation will result in solidity's default
    /// value for the method's return type. This method allows specifying
    /// a custom return value.
    ///
    /// This method will overwrite any return value or callback
    /// that was set before.
    pub fn returns(self, returns: R) -> Self {
        todo!()
    }

    /// Sets callback function that will be used to calculate return value
    /// of the method. This function accepts a tuple of method's arguments
    /// and returns method's result or [`Err`] if transaction
    /// should be reverted.
    pub fn returns_fn(self, returns: impl Fn(P) -> Result<R, String> + Send + 'static) -> Self {
        todo!()
    }

    /// Sets callback function that will be used to calculate return value
    /// of the method. This function accepts a [call context] and a tuple
    /// of method's arguments and returns method's result or [`Err`]
    /// if transaction should be reverted.
    pub fn returns_fn_ctx(
        self,
        returns: impl Fn(&CallContext, P) -> Result<R, String> + Send + 'static,
    ) -> Self {
        todo!()
    }

    /// Sets return value of the method to an error, meaning that calls to this
    /// expectation result in reverted transaction.
    pub fn returns_error(self, error: String) -> Self {
        todo!()
    }

    /// Sets return value of the method to a default value for its solidity type.
    /// See [`returns`] for more info.
    pub fn returns_default(self) -> Self {
        todo!()
    }
}

/// Information about method call that's being processed.
pub struct CallContext {
    /// If `true`, this is a view call, otherwise this is a transaction.
    pub is_view_call: bool,

    /// Account that issued a view call or a transaction.
    ///
    /// Can be zero in case of a view call.
    pub from: Address,

    /// Address of the current contract.
    pub to: Address,

    /// Current nonce of the account that issued a view call or a transaction.
    pub nonce: U256,

    /// Maximum gas amount that this operation is allowed to spend.
    ///
    /// Mock node does not simulate gas consumption, so this value does not
    /// affect anything if you don't check it in your test code.
    pub gas: U256,

    /// Gas price for this view call or transaction.
    ///
    /// Mock node does not simulate gas consumption, so this value does not
    /// affect anything if you don't check it in your test code.
    pub gas_price: U256,

    /// Amount of ETH that's transferred with the call.
    ///
    /// This value is only non-zero if the method is payable.
    pub value: U256,
}
