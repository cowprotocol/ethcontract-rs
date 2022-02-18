//! Helpers to parse RPC call arguments.

use ethcontract::json::{from_value, Value};
use ethcontract::jsonrpc::serde::Deserialize;
use ethcontract::web3::types::BlockNumber;
use std::fmt::Display;

/// A helper to parse RPC call arguments.
///
/// RPC call arguments are parsed from JSON string into an array
/// of `Value`s before they're passed to method handlers.
/// This struct helps to transform `Value`s into actual rust types,
/// while also handling optional arguments.
pub struct Parser {
    name: &'static str,
    args: Vec<Value>,
    current: usize,
}

impl Parser {
    /// Create new parser.
    pub fn new(name: &'static str, args: Vec<Value>) -> Self {
        Parser {
            name,
            args,
            current: 0,
        }
    }

    /// Parse an argument.
    pub fn arg<T: for<'b> Deserialize<'b>>(&mut self) -> T {
        if let Some(arg) = self.args.get_mut(self.current) {
            self.current += 1;
            let val = from_value(std::mem::take(arg));
            self.res(val)
        } else {
            panic!("not enough arguments for rpc call {:?}", self.name);
        }
    }

    /// Parse an optional argument, return `None` if arguments list is exhausted.
    pub fn arg_opt<T: for<'b> Deserialize<'b>>(&mut self) -> Option<T> {
        if self.current < self.args.len() {
            Some(self.arg())
        } else {
            None
        }
    }

    /// Parse an optional argument with a block number.
    ///
    /// Since [`BlockNumber`] does not implement [`Deserialize`],
    /// we can't use [`arg`] to parse it, so we use this helper method.
    pub fn block_number_opt(&mut self) -> Option<BlockNumber> {
        let value = self.arg_opt();
        value.map(|value| self.parse_block_number(value))
    }

    /// Finish parsing arguments.
    ///
    /// If there are unparsed arguments, report them as extraneous.
    pub fn done(self) {
        // nothing here, actual check is in the `drop` method.
    }

    // Helper for parsing block numbers.
    fn parse_block_number(&self, value: Value) -> BlockNumber {
        match value.as_str() {
            Some("latest") => BlockNumber::Latest,
            Some("earliest") => BlockNumber::Earliest,
            Some("pending") => BlockNumber::Pending,
            Some(number) => BlockNumber::Number(self.res(number.parse())),
            None => self.err("block number should be a string"),
        }
    }

    // Unwraps `Result`, adds info with current arg index and rpc name.
    fn res<T, E: Display>(&self, res: Result<T, E>) -> T {
        res.unwrap_or_else(|err| self.err(err))
    }

    // Panics, adds info with current arg index and rpc name.
    fn err<E: Display>(&self, err: E) -> ! {
        panic!(
            "argument {} for rpc call {:?} is invalid: {}",
            self.current, self.name, err
        )
    }
}

impl Drop for Parser {
    fn drop(&mut self) {
        assert!(
            std::thread::panicking() || self.current >= self.args.len(),
            "too many arguments for rpc call {:?}",
            self.name
        );
    }
}
