//! Implementation of a transport for testing purposes. This is largely based on
//! the `rust-web3` `TestTransport` type with some modifications.

use jsonrpc_core::{Call, Value};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use web3::error::Error;
use web3::futures::future::{self, FutureResult};
use web3::helpers;
use web3::{RequestId, Transport};

/// Type alias for request method and value pairs
type Requests = Vec<(String, Vec<Value>)>;

/// Test transport
#[derive(Debug, Default, Clone)]
pub struct TestTransport {
    asserted: usize,
    requests: Rc<RefCell<Requests>>,
    responses: Rc<RefCell<VecDeque<Value>>>,
}

impl Transport for TestTransport {
    type Out = FutureResult<Value, Error>;

    fn prepare(&self, method: &str, params: Vec<Value>) -> (RequestId, Call) {
        let request = helpers::build_request(1, method, params.clone());
        self.requests.borrow_mut().push((method.into(), params));
        (self.requests.borrow().len(), request)
    }

    fn send(&self, id: RequestId, request: Call) -> Self::Out {
        match self.responses.borrow_mut().pop_front() {
            Some(response) => future::ok(response),
            None => {
                println!("Unexpected request (id: {:?}): {:?}", id, request);
                future::err(Error::Unreachable)
            }
        }
    }
}

impl TestTransport {
    /// Create a new test transport instance.
    pub fn new() -> Self {
        Default::default()
    }

    /// Add a response to an eventual request.
    pub fn add_response(&mut self, value: Value) {
        self.responses.borrow_mut().push_back(value);
    }

    /// Assert that a request was made.
    pub fn assert_request(&mut self, method: &str, params: &[Value]) {
        let idx = self.asserted;
        self.asserted += 1;

        let (m, p) = self
            .requests
            .borrow()
            .get(idx)
            .expect("Expected result.")
            .clone();
        assert_eq!(&m, method);
        assert_eq!(&p[..], params);
    }

    /// Assert that there are no more pending requests.
    pub fn assert_no_more_requests(&self) {
        let requests = self.requests.borrow();
        assert_eq!(
            self.asserted,
            requests.len(),
            "Expected no more requests, got: {:?}",
            &requests[self.asserted..]
        );
    }
}
