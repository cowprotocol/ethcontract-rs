//! Implementation details of mock node.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ethcontract::common::abi::{Function, Token};
use ethcontract::common::hash::H32;
use ethcontract::common::{Abi, FunctionExt};
use ethcontract::tokens::Tokenize;
use ethcontract::web3::types::TransactionReceipt;
use ethcontract::web3::RequestId;
use ethcontract::{Address, H160, H256};

use crate::range::TimesRange;
use crate::CallContext;
use std::any::Any;

mod default;
mod parse;
mod sign;
mod transaction;

/// Mock transport.
#[derive(Clone)]
pub struct MockTransport {
    /// Mutable state.
    state: Arc<Mutex<MockTransportState>>,
}

/// Internal transport state, protected by a mutex.
struct MockTransportState {
    /// Chain ID.
    chain_id: u64,

    /// Current gas price.
    gas_price: u64,

    /// This counter is used to keep track of prepared calls.
    request_id: RequestId,

    /// This counter is used to keep track of mined blocks.
    block: u64,

    /// This counter is used to generate contract addresses.
    address: u64,

    /// Nonce for account.
    nonce: HashMap<Address, u64>,

    /// Deployed mocked contracts.
    contracts: HashMap<Address, Contract>,

    /// Receipts for already performed transactions.
    receipts: HashMap<H256, TransactionReceipt>,
}

impl MockTransport {
    /// Creates a new transport.
    pub fn new(chain_id: u64) -> Self {
        MockTransport {
            state: Arc::new(Mutex::new(MockTransportState {
                chain_id,
                gas_price: 1,
                request_id: 0,
                block: 0,
                address: 0,
                nonce: HashMap::new(),
                contracts: HashMap::new(),
                receipts: HashMap::new(),
            })),
        }
    }

    /// Deploys a new contract with the given ABI.
    pub fn deploy(&self, abi: &Abi) -> Address {
        let mut state = self.state.lock().unwrap();

        state.address += 1;
        let address = H160::from_low_u64_be(state.address);

        state.contracts.insert(address, Contract::new(address, abi));

        address
    }

    pub fn update_gas_price(&self, gas_price: u64) {
        let mut state = self.state.lock().unwrap();
        state.gas_price = gas_price;
    }
}

impl std::fmt::Debug for MockTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MockTransport")
    }
}

/// A mocked contract instance.
struct Contract {
    address: Address,
    methods: HashMap<H32, Method>,
}

impl Contract {
    fn new(address: Address, abi: &Abi) -> Self {
        let mut methods = HashMap::new();

        for functions in abi.functions.values() {
            for function in functions {
                methods.insert(function.selector(), Method::new(address, function.clone()));
            }
        }

        Contract { address, methods }
    }
}

struct Method {
    /// Description for this method.
    description: String,

    /// ABI of this method.
    function: Function,

    /// Incremented whenever `expectations` vector is cleared to invalidate
    /// expectations API handle.
    generation: usize,

    /// Expectation for this method.
    expectations: Vec<Box<dyn ExpectationApi>>,
}

impl Method {
    /// Creates new method.
    fn new(address: Address, function: Function) -> Self {
        let description = format!("{:?} on contract {:#x}", function.abi_signature(), address);

        Method {
            description,
            function,
            generation: 0,
            expectations: Vec::new(),
        }
    }
}

trait ExpectationApi: Send {
    /// Convert this expectation to `Any` for downcast.
    fn as_any(&mut self) -> &mut dyn Any;
}

struct Expectation<P: Tokenize + Send + 'static, R: Tokenize + Send + 'static> {
    /// How many times should this expectation be called.
    times: TimesRange,

    /// How many times was it actually called.
    used: usize,

    /// Indicates that predicate for this expectation has been called at least
    /// once. Expectations shouldn't be changed after that happened.
    checked: bool,

    /// How many blocks should node skip for confirmation to be successful.
    confirmations: u64,

    /// Only consider this expectation if predicate returns `true`.
    predicate: Predicate<P>,

    /// Should this expectation match view calls?
    allow_calls: bool,

    /// Should this expectation match transactions?
    allow_transactions: bool,

    /// Function to generate method's return value.
    returns: Returns<P, R>,

    /// Handle for when this expectation belongs to a sequence.
    sequence: Option<mockall::SeqHandle>,
}

impl<P: Tokenize + Send + 'static, R: Tokenize + Send + 'static> Expectation<P, R> {
    fn new() -> Self {
        Expectation {
            times: TimesRange::default(),
            used: 0,
            checked: false,
            confirmations: 0,
            predicate: Predicate::None,
            allow_calls: true,
            allow_transactions: true,
            returns: Returns::Default,
            sequence: None,
        }
    }
}

impl<P: Tokenize + Send + 'static, R: Tokenize + Send + 'static> ExpectationApi
    for Expectation<P, R>
{
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

enum Predicate<P: Tokenize + Send + 'static> {
    None,
    Predicate(Box<dyn predicates::Predicate<P> + Send>),
    Function(Box<dyn Fn(&P) -> bool + Send>),
    TxFunction(Box<dyn Fn(&CallContext, &P) -> bool + Send>),
}

enum Returns<P: Tokenize + Send + 'static, R: Tokenize + Send + 'static> {
    Default,
    Error(String),
    Const(Token),
    Function(Box<dyn Fn(P) -> Result<R, String> + Send>),
    TxFunction(Box<dyn Fn(&CallContext, P) -> Result<R, String> + Send>),
}
