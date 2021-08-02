//! Implementation details of mock node.

use std::collections::HashMap;
use std::convert::TryFrom;
use std::future::ready;
use std::sync::{Arc, Mutex};

use ethcontract::common::abi::{Function, StateMutability, Token};
use ethcontract::common::hash::H32;
use ethcontract::common::{Abi, FunctionExt};
use ethcontract::jsonrpc::serde::Serialize;
use ethcontract::jsonrpc::serde_json::to_value;
use ethcontract::jsonrpc::{Call, MethodCall, Params, Value};
use ethcontract::tokens::Tokenize;
use ethcontract::web3::types::{
    Bytes, CallRequest, TransactionReceipt, TransactionRequest, U256, U64,
};
use ethcontract::web3::{helpers, Error, RequestId, Transport};
use ethcontract::{Address, BlockNumber, H160, H256};
use parse::Parser;
use sign::verify;

use crate::details::transaction::TransactionResult;
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

    pub fn expect<P: Tokenize + Send + 'static, R: Tokenize + Send + 'static>(
        &self,
        address: Address,
        signature: H32,
    ) -> (usize, usize) {
        let mut state = self.state.lock().unwrap();
        let method = state.method(address, signature);
        method.expect::<P, R>()
    }
}

impl MockTransportState {
    /// Returns contract at the given address, panics if contract does not exist.
    fn contract(&mut self, address: Address) -> &mut Contract {
        match self.contracts.get_mut(&address) {
            Some(contract) => contract,
            None => panic!("there is no mocked contract with address {:#x}", address),
        }
    }

    /// Returns contract's method.
    fn method(&mut self, address: Address, signature: H32) -> &mut Method {
        self.contract(address).method(signature)
    }
}

impl Transport for MockTransport {
    type Out = std::future::Ready<Result<Value, Error>>;

    /// Prepares an RPC call for given method with parameters.
    ///
    /// We don't have to deal with network issues, so we are relaxed about
    /// request IDs, idempotency checks and so on.
    fn prepare(&self, method: &str, params: Vec<Value>) -> (RequestId, Call) {
        let mut state = self.state.lock().unwrap();

        let id = state.request_id;
        state.request_id += 1;

        let request = helpers::build_request(id, method, params);

        (id, request)
    }

    /// Executes a prepared RPC call.
    fn send(&self, _: RequestId, request: Call) -> Self::Out {
        let MethodCall { method, params, .. } = match request {
            Call::MethodCall(method_call) => method_call,
            Call::Notification(_) => panic!("rpc notifications are not supported"),
            _ => panic!("unknown or invalid rpc call type"),
        };

        let params = match params {
            Params::None => Vec::new(),
            Params::Array(array) => array,
            Params::Map(_) => panic!("passing arguments by map is not supported"),
        };

        let result = match method.as_str() {
            "eth_blockNumber" => {
                let name = "eth_blockNumber";
                self.block_number(Parser::new(name, params))
            }
            "eth_chainId" => {
                let name = "eth_chainId";
                self.chain_id(Parser::new(name, params))
            }
            "eth_getTransactionCount" => {
                let name = "eth_getTransactionCount";
                self.transaction_count(Parser::new(name, params))
            }
            "eth_gasPrice" => {
                let name = "eth_gasPrice";
                self.gas_price(Parser::new(name, params))
            }
            unsupported => panic!("mock node does not support rpc method {:?}", unsupported),
        };

        ready(result)
    }
}

impl MockTransport {
    fn block_number(&self, args: Parser) -> Result<Value, Error> {
        args.done();

        let state = self.state.lock().unwrap();
        Self::ok(&U64::from(state.block))
    }

    fn chain_id(&self, args: Parser) -> Result<Value, Error> {
        args.done();

        let state = self.state.lock().unwrap();
        Self::ok(&U256::from(state.chain_id))
    }

    fn transaction_count(&self, mut args: Parser) -> Result<Value, Error> {
        let address: Address = args.arg();
        let block: Option<BlockNumber> = args.block_number_opt();
        args.done();

        let block = block.unwrap_or(BlockNumber::Pending);
        let state = self.state.lock().unwrap();
        let transaction_count = match block {
            BlockNumber::Earliest => 0,
            BlockNumber::Number(n) if n == 0.into() => 0,
            BlockNumber::Number(n) if n != state.block.into() => {
                panic!("mock node does not support returning transaction count for specific block number");
            }
            _ => state.nonce.get(&address).copied().unwrap_or(0),
        };
        Self::ok(&U256::from(transaction_count))
    }

    fn gas_price(&self, args: Parser) -> Result<Value, Error> {
        args.done();

        let state = self.state.lock().unwrap();
        Self::ok(&U256::from(state.gas_price))
    }

    fn ok<T: Serialize>(t: T) -> Result<Value, Error> {
        Ok(to_value(t).unwrap())
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

    fn method(&mut self, signature: H32) -> &mut Method {
        match self.methods.get_mut(&signature) {
            Some(method) => method,
            None => panic!(
                "contract {:#x} doesn't have method with signature 0x{}",
                self.address,
                hex::encode(signature)
            ),
        }
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

    /// Adds new expectation.
    fn expect<P: Tokenize + Send + 'static, R: Tokenize + Send + 'static>(
        &mut self,
    ) -> (usize, usize) {
        let index = self.expectations.len();
        self.expectations.push(Box::new(Expectation::<P, R>::new()));
        (index, self.generation)
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
