#![deny(missing_docs, unsafe_code)]

//! This crate allows emulating ethereum node with a limited number
//! of supported RPC calls, enabling you to mock ethereum contracts.
//!
//! Create a new deployment using the [`Mock::deploy`] function.
//!
//! Configure contract's behaviour using [`Contract::expect_transaction`]
//! and [`Contract::expect_call`].
//!
//! Finally, create an ethcontract's [`Instance`] by calling [`Contract::instance`],
//! then use said instance in your tests.
//!
//! # Example
//!
//! Let's mock [voting contract] from solidity examples.
//!
//! First, we create a mock node and deploy a new mocked contract:
//!
//! ```
//! # include!("test/doctest/common.rs");
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let abi = voting_abi();
//! let mock = Mock::new(/* chain_id = */ 1337);
//! let contract = mock.deploy(abi);
//! # Ok(())
//! # }
//! ```
//!
//! Then we set up expectations for method calls:
//!
//! ```
//! # include!("test/doctest/common.rs");
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let abi = voting_abi();
//! # let account = account_for("Alice");
//! # let mock = Mock::new(1337);
//! # let contract = mock.deploy(abi);
//! // We'll need to know method signatures and types.
//! let vote: Signature<(U256,), ()> = [1, 33, 185, 63].into();
//! let winning_proposal: Signature<(), U256> = [96, 159, 241, 189].into();
//!
//! // We expect some transactions calling the `vote` method.
//! contract
//!     .expect_transaction(vote);
//!
//! // We also expect calls to `winning_proposal` that will return
//! // a value of `1`.
//! contract
//!     .expect_call(winning_proposal)
//!     .returns(1.into());
//! # Ok(())
//! # }
//! ```
//!
//! Finally, we create a dynamic instance and work with it as usual:
//!
//! ```
//! # include!("test/doctest/common.rs");
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let abi = voting_abi();
//! # let account = account_for("Alice");
//! # let mock = Mock::new(1337);
//! # let contract = mock.deploy(abi);
//! # let vote: Signature<(U256,), ()> = [1, 33, 185, 63].into();
//! # let winning_proposal: Signature<(), U256> = [96, 159, 241, 189].into();
//! # contract.expect_transaction(vote);
//! # contract.expect_call(winning_proposal).returns(1.into());
//! let instance = contract.instance();
//!
//! instance
//!     .method(vote, (1.into(),))?
//!     .from(account)
//!     .send()
//!     .await?;
//!
//! let winning_proposal_index = instance
//!     .view_method(winning_proposal, ())?
//!     .call()
//!     .await?;
//! assert_eq!(winning_proposal_index, 1.into());
//! # Ok(())
//! # }
//! ```
//!
//! # Describing expectations
//!
//! The mocked contracts have an interface similar to the one
//! of the [`mockall`] crate.
//!
//! For each contract's method that you expect to be called during a test,
//! call [`Contract::expect_transaction`] or [`Contract::expect_call`]
//! and set up the created [`Expectation`] with functions such as [`returns`],
//! [`times`], [`in_sequence`]. For greater flexibility, you can have
//! multiple expectations attached to the same method.
//!
//! See [`Expectation`] for more info and examples.
//!
//! # Interacting with mocked contracts
//!
//! After contract's behaviour is programmed, you can call
//! [`Contract::instance`] to create an ethcontract's [`Instance`].
//!
//! You can also get contract's address and send RPC calls directly
//! through [`web3`].
//!
//! Specifically, mock node supports `eth_call`, `eth_sendRawTransaction`,
//! and `eth_getTransactionReceipt`.
//!
//! At the moment, mock node can't sign transactions on its own,
//! so `eth_sendTransaction` is not supported. Also, deploying contracts
//! via `eth_sendRawTransaction` is not possible yet.
//!
//! # Mocking generated contracts
//!
//! Overall, generated contracts are similar to the dynamic ones:
//! they are deployed with [`Mock::deploy`] and configured with
//! [`Contract::expect_call`] and [`Contract::expect_transaction`].
//!
//! You can get generated contract's ABI using the `raw_contract` function.
//!
//! Generated [method signatures] are available through the `signatures`
//! function.
//!
//! Finally, type-safe instance can be created using the `at` method.
//!
//! Here's an example of mocking an ERC20-compatible contract.
//!
//! First, we create a mock node and deploy a new mocked contract:
//!
//! ```
//! # include!("test/doctest/common.rs");
//! # /*
//! ethcontract::contract!("ERC20.json");
//! # */
//! # ethcontract::contract!(
//! #     "../examples/truffle/build/contracts/IERC20.json",
//! #     contract = IERC20 as ERC20,
//! # );
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mock = Mock::new(/* chain_id = */ 1337);
//! let contract = mock.deploy(ERC20::raw_contract().abi.clone());
//! # Ok(())
//! # }
//! ```
//!
//! Then we set up expectations using the generated method signatures:
//!
//! ```
//! # include!("test/doctest/common.rs");
//! # ethcontract::contract!(
//! #     "../examples/truffle/build/contracts/IERC20.json",
//! #     contract = IERC20 as ERC20,
//! # );
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let account = account_for("Alice");
//! # let recipient = address_for("Bob");
//! # let mock = Mock::new(1337);
//! # let contract = mock.deploy(ERC20::raw_contract().abi.clone());
//! contract
//!     .expect_transaction(ERC20::signatures().transfer())
//!     .once()
//!     .returns(true);
//! # let instance = ERC20::at(&mock.web3(), contract.address());
//! # instance.transfer(recipient, 100.into()).from(account).send().await?;
//! # Ok(())
//! # }
//! ```
//!
//! Finally, we use mock contract's address to interact with the mock node:
//!
//! ```
//! # include!("test/doctest/common.rs");
//! # ethcontract::contract!(
//! #     "../examples/truffle/build/contracts/IERC20.json",
//! #     contract = IERC20 as ERC20,
//! # );
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let account = account_for("Alice");
//! # let recipient = address_for("Bob");
//! # let mock = Mock::new(1337);
//! # let contract = mock.deploy(ERC20::raw_contract().abi.clone());
//! # contract.expect_transaction(ERC20::signatures().transfer());
//! let instance = ERC20::at(&mock.web3(), contract.address());
//! instance
//!     .transfer(recipient, 100.into())
//!     .from(account)
//!     .send()
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Mocking gas and gas estimation
//!
//! Mock node allows you to customize value returned from `eth_gasPrice`
//! RPC call. Use [`Mock::update_gas_price`] to set a new gas price.
//!
//! Estimating gas consumption with `eth_estimateGas` is not supported at the
//! moment. For now, calls to `eth_estimateGas` always return `1`.
//!
//! [`web3-rs`]: ethcontract::web3
//! [`web3`]: ethcontract::web3
//! [`expect_call`]: Contract::expect_call
//! [`expect_transaction`]: Contract::expect_transaction
//! [`returns`]: Expectation::returns
//! [`times`]: Expectation::times
//! [`in_sequence`]: Expectation::in_sequence
//! [`Instance`]: ethcontract::Instance
//! [voting contract]: https://docs.soliditylang.org/en/v0.8.6/solidity-by-example.html#voting
//! [method signatures]: Signature

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
pub mod utils;

#[cfg(test)]
mod test;

/// Mock ethereum node.
///
/// This struct implements a virtual ethereum node with a limited number
/// of supported RPC calls. You can interact with it via the standard
/// transport from `web3`.
///
/// The main feature of this struct is deploying mocked contracts
/// and interacting with them. Create new mocked contract with a call
/// to [`deploy`] function. Then use the returned struct to set up
/// expectations on contract methods, get deployed contract's address
/// and [`Instance`] and make actual calls to it.
///
/// Deploying contracts with an RPC call is not supported at the moment.
///
/// [`deploy`]: Mock::deploy
/// [`Instance`]: ethcontract::Instance
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
    ///
    /// Sometimes its useful to validate all expectations mid-test,
    /// throw them away, and add new ones. That’s what checkpoints do.
    /// See [mockall documentation] for more info.
    ///
    /// Note that all expectations returned from [`Contract::expect`] method
    /// become invalid after checkpoint. Modifying them will result in panic.
    ///
    /// [mockall documentation]: https://docs.rs/mockall/#checkpoints
    pub fn checkpoint(&self) {
        self.transport.checkpoint();
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
    ///
    /// Generic parameters are used to specify which rust types should be used
    /// to encode and decode method's arguments and return value. If you're
    /// using generated contracts, they will be inferred automatically.
    /// If not, you may have to specify them manually.
    ///
    /// # Notes
    ///
    /// Expectations generated by this method will allow both view calls
    /// and transactions. This is usually undesired, so prefer using
    /// [`expect_call`] and [`expect_transaction`] instead.
    ///
    /// [`expect_call`]: Contract::expect_call
    /// [`expect_transaction`]: Contract::expect_transaction
    pub fn expect<P: Tokenize + Send + 'static, R: Tokenize + Send + 'static>(
        &self,
        signature: impl Into<Signature<P, R>>,
    ) -> Expectation<P, R> {
        let signature = signature.into().into_inner();
        let (index, generation) = self.transport.expect::<P, R>(self.address, signature);
        Expectation {
            transport: self.transport.clone(),
            address: self.address,
            signature,
            index,
            generation,
            _ph: PhantomData,
        }
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
    ///
    /// Sometimes its useful to validate all expectations mid-test,
    /// throw them away, and add new ones. That’s what checkpoints do.
    /// See [mockall documentation] for more info.
    ///
    /// Note that all expectations returned from [`expect`] method
    /// become invalid after checkpoint. Modifying them will result in panic.
    ///
    /// [mockall documentation]: https://docs.rs/mockall/#checkpoints
    /// [`expect`]: Contract::expect
    pub fn checkpoint(&self) {
        self.transport.contract_checkpoint(self.address);
    }
}

/// Expectation for contract method.
///
/// A method could have multiple expectations associated with it.
/// Each expectation specifies how the method should be called, how many times,
/// with what arguments, etc.
///
/// When a method gets called, mock node determines if the call is expected
/// or not. It goes through each of the method's expectations in order they
/// were created, searching for the first expectation that matches the call.
///
/// If a suitable expectation is found, it is used to determine method's
/// return value and other transaction properties. If not, the call
/// is considered unexpected, and mock node panics.
///
/// To determine if a particular expectation should be used for the given call,
/// mock node uses two of the expectation's properties:
///
/// - [`predicate`] checks if method's arguments and transaction properties
///   match a certain criteria;
/// - [times limiter] is used to limit number of times a single expectation
///   can be used.
///
/// To determine result of a method call, [`returns`] property is used.
///
/// # Notes
///
/// Expectations can't be changed after they were used. That is, if you try
/// to modify an expectation after making any calls to its contract method,
/// mock node will panic. This happens because modifying an already-used
/// expectation may break node's internal state. Adding new expectations
/// at any time is fine, though.
///
/// [`predicate`]: Expectation::predicate
/// [times limiter]: Expectation::times
/// [`returns`]: Expectation::returns
pub struct Expectation<P: Tokenize + Send + 'static, R: Tokenize + Send + 'static> {
    transport: details::MockTransport,
    address: Address,
    signature: H32,
    index: usize,
    generation: usize,
    _ph: PhantomData<(P, R)>,
}

impl<P: Tokenize + Send + 'static, R: Tokenize + Send + 'static> Expectation<P, R> {
    /// Specifies how many times this expectation can be called.
    ///
    /// By default, each expectation can be called any number of times,
    /// including zero. This method allows specifying a more precise range.
    ///
    /// For example, use `times(1)` to indicate that the expectation
    /// should be called exactly [`once`]. Or use `times(1..)` to indicate
    /// that it should be called at least once. Any range syntax is accepted.
    ///
    /// If the expectation gets called less that the specified number
    /// of times, the test panics.
    ///
    /// If it gets called enough number of times, expectation is considered
    /// satisfied. It becomes inactive and is no longer checked when processing
    /// new method calls.
    ///
    /// # Examples
    ///
    /// Consider a method with two expectations:
    ///
    /// ```
    /// # include!("test/doctest/common.rs");
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let contract = contract();
    /// # let signature = signature();
    /// contract
    ///     .expect_call(signature)
    ///     .times(1..=2);
    /// contract
    ///     .expect_call(signature);
    /// # contract.instance().view_method(signature, (0, 0))?.call().await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// The first two calls to this method will be dispatched to the first
    /// expectation. Then first expectation will become satisfied, and all
    /// other calls will be dispatched to the second one.
    ///
    /// # Notes
    ///
    /// When expectation becomes satisfied, previous expectations
    /// are not altered and may still be unsatisfied. This is important
    /// when you have expectations with predicates:
    ///
    /// ```
    /// # include!("test/doctest/common.rs");
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let contract = contract();
    /// # let signature = signature();
    /// contract
    ///     .expect_call(signature)
    ///     .predicate_fn(|(a, b)| a == b)
    ///     .times(1..=2);
    /// contract
    ///     .expect_call(signature)
    ///     .times(1);
    /// contract
    ///     .expect_call(signature)
    ///     .times(..);
    /// # contract.instance().view_method(signature, (0, 0))?.call().await?;
    /// # contract.instance().view_method(signature, (0, 1))?.call().await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Here, first expectation can be called one or two times, second
    /// expectation can be called exactly once, and third expectation
    /// can be called arbitrary number of times.
    ///
    /// Now, consider the following sequence of calls:
    ///
    /// ```
    /// # include!("test/doctest/common.rs");
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let contract = contract();
    /// # let signature = signature();
    /// # let instance = contract.instance();
    /// # contract.expect(signature);
    /// instance
    ///     .method(signature, (1, 1))?
    ///     .call()
    ///     .await?;
    /// instance
    ///     .method(signature, (2, 3))?
    ///     .call()
    ///     .await?;
    /// instance
    ///     .method(signature, (5, 5))?
    ///     .call()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// First call gets dispatched to the first expectation. Second call
    /// can't be dispatched to the first expectation because of its predicate,
    /// so it gets dispatched to the second one. Now, one may assume that
    /// the third call will be dispatched to the third expectation. However,
    /// first expectation can be called one more time, so it is not satisfied
    /// yet. Because of this, third call gets dispatched
    /// to the first expectation.
    ///
    /// [`once`]: Expectation::once
    pub fn times(self, times: impl Into<TimesRange>) -> Self {
        self.transport.times::<P, R>(
            self.address,
            self.signature,
            self.index,
            self.generation,
            times.into(),
        );
        self
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
    ///
    /// By default, expectations may be matched in any order. If a stricter
    /// order is required, you can use sequences. See [mockall documentation]
    /// for more info.
    ///
    /// # Limitations
    ///
    /// An expectation can be in one sequence only.
    ///
    /// Also, an expectation should have [`times`] limit set to an exact
    /// number of calls, i.e., [`once`], two times, and so on.
    ///
    /// [mockall documentation]: https://docs.rs/mockall/#sequences
    /// [`times`]: Expectation::times
    /// [`once`]: Expectation::once
    pub fn in_sequence(self, sequence: &mut mockall::Sequence) -> Self {
        self.transport.in_sequence::<P, R>(
            self.address,
            self.signature,
            self.index,
            self.generation,
            sequence,
        );
        self
    }

    /// Sets number of blocks that should be mined on top of the transaction
    /// block. This method can be useful when there are custom transaction
    /// confirmation settings.
    pub fn confirmations(self, confirmations: u64) -> Self {
        self.transport.confirmations::<P, R>(
            self.address,
            self.signature,
            self.index,
            self.generation,
            confirmations,
        );
        self
    }

    /// Sets predicate for this expectation.
    ///
    /// If method has multiple expectations, they are checked one-by-one,
    /// in order they were created. First expectation with a predicate that
    /// matches method's parameters is called.
    ///
    /// This method accepts a tuple of predicates, one predicate
    /// for each parameter.
    ///
    /// This method will overwrite any predicate that was set before.
    ///
    /// # Examples
    ///
    /// ```
    /// # include!("test/doctest/common.rs");
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let contract = contract();
    /// # let signature = signature();
    /// contract
    ///     .expect_call(signature)
    ///     .predicate((predicate::eq(1), predicate::eq(1)))
    ///     .returns(1);
    /// contract
    ///     .expect_call(signature)
    ///     .predicate_fn(|(a, b)| a > b)
    ///     .returns(2);
    /// contract
    ///     .expect_call(signature)
    ///     .returns(3);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Here, we have three expectations, resulting in the following behaviour.
    /// If both arguments are equal to `1`, method returns `1`.
    /// Otherwise, if the first argument is greater than the second one, method
    /// returns `2`. Otherwise, it returns `3`.
    ///
    /// # Notes
    ///
    /// Having multiple predicates shines for complex setups that involve
    /// [call sequences] and [limiting number of expectation uses].
    /// For simpler setups like the one above, [`returns_fn`] may be more
    /// clear and concise, and also more efficient.
    ///
    /// [call sequences]: Expectation::in_sequence
    /// [limiting number of expectation uses]: Expectation::times
    /// [`returns_fn`]: Expectation::returns_fn
    pub fn predicate<T>(self, pred: T) -> Self
    where
        T: TuplePredicate<P> + Send + 'static,
        <T as predicate::TuplePredicate<P>>::P: Send,
    {
        self.transport.predicate::<P, R>(
            self.address,
            self.signature,
            self.index,
            self.generation,
            Box::new(pred.into_predicate()),
        );
        self
    }

    /// Sets predicate function for this expectation. This function accepts
    /// a tuple of method's arguments and returns `true` if this
    /// expectation should be called. See [`predicate`] for more info.
    ///
    /// This method will overwrite any predicate that was set before.
    ///
    /// [`predicate`]: Expectation::predicate
    pub fn predicate_fn(self, pred: impl Fn(&P) -> bool + Send + 'static) -> Self {
        self.transport.predicate_fn::<P, R>(
            self.address,
            self.signature,
            self.index,
            self.generation,
            Box::new(pred),
        );
        self
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
        self.transport.predicate_fn_ctx::<P, R>(
            self.address,
            self.signature,
            self.index,
            self.generation,
            Box::new(pred),
        );
        self
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
        self.transport.allow_calls::<P, R>(
            self.address,
            self.signature,
            self.index,
            self.generation,
            allow_calls,
        );
        self
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
        self.transport.allow_transactions::<P, R>(
            self.address,
            self.signature,
            self.index,
            self.generation,
            allow_transactions,
        );
        self
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
        self.transport.returns::<P, R>(
            self.address,
            self.signature,
            self.index,
            self.generation,
            returns,
        );
        self
    }

    /// Sets callback function that will be used to calculate return value
    /// of the method. This function accepts a tuple of method's arguments
    /// and returns method's result or [`Err`] if transaction
    /// should be reverted.
    ///
    /// A callback set by this method will be called even if its return value
    /// is unused, such as when processing a transaction. This means that
    /// callback can be used to further check method's parameters, perform
    /// asserts and invoke other logic.
    ///
    /// This method will overwrite any return value or callback
    /// that was set before.
    ///
    /// See [`returns`] for more info.
    ///
    /// [`returns`]: Expectation::returns
    pub fn returns_fn(self, returns: impl Fn(P) -> Result<R, String> + Send + 'static) -> Self {
        self.transport.returns_fn::<P, R>(
            self.address,
            self.signature,
            self.index,
            self.generation,
            Box::new(returns),
        );
        self
    }

    /// Sets callback function that will be used to calculate return value
    /// of the method. This function accepts a [call context] and a tuple
    /// of method's arguments and returns method's result or [`Err`]
    /// if transaction should be reverted.
    ///
    /// A callback set by this method will be called even if its return value
    /// is unused, such as when processing a transaction. This means that
    /// callback can be used to further check method's parameters, perform
    /// asserts and invoke other logic.
    ///
    /// This method will overwrite any return value or callback
    /// that was set before.
    ///
    /// See [`returns`] for more info.
    ///
    /// [call context]: CallContext
    /// [`returns`]: Expectation::returns
    pub fn returns_fn_ctx(
        self,
        returns: impl Fn(&CallContext, P) -> Result<R, String> + Send + 'static,
    ) -> Self {
        self.transport.returns_fn_ctx::<P, R>(
            self.address,
            self.signature,
            self.index,
            self.generation,
            Box::new(returns),
        );
        self
    }

    /// Sets return value of the method to an error, meaning that calls to this
    /// expectation result in reverted transaction.
    ///
    /// This method will overwrite any return value or callback
    /// that was set before.
    ///
    /// See [`returns`] for more info.
    ///
    /// [`returns`]: Expectation::returns
    pub fn returns_error(self, error: String) -> Self {
        self.transport.returns_error::<P, R>(
            self.address,
            self.signature,
            self.index,
            self.generation,
            error,
        );
        self
    }

    /// Sets return value of the method to a default value for its solidity type.
    /// See [`returns`] for more info.
    ///
    /// This method will overwrite any return value or callback
    /// that was set before.
    ///
    /// Note that this method doesn't use [`Default`] trait for `R`. Instead,
    /// it constructs default value according to solidity rules.
    ///
    /// [`returns`]: Expectation::returns
    pub fn returns_default(self) -> Self {
        self.transport.returns_default::<P, R>(
            self.address,
            self.signature,
            self.index,
            self.generation,
        );
        self
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
