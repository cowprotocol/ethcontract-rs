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

/// Request
#[derive(Debug)]
pub struct Request {
    pub method: String,
    pub params: Vec<Value>,
}

/// RequestResponse
#[derive(Debug)]
pub struct RequestResponse {
    request: Request,
    response: Value,
}

/// Test transport
#[derive(Debug, Default, Clone)]
pub struct TestTransport {
    expected_request_responses: Rc<RefCell<VecDeque<RequestResponse>>>,
}

impl Transport for TestTransport {
    type Out = FutureResult<Value, Error>;

    fn prepare(&self, method: &str, params: Vec<Value>) -> (RequestId, Call) {
        let request = helpers::build_request(1, method, params.clone());
        let id = self.expected_request_responses.borrow().len();
        let expected = self
            .expected_request_responses
            .borrow()
            .get(0)
            .ok_or_else(|| format!("unexpected request: {:?}", request))
            .unwrap();
        assert_eq!(expected.request.method, method);
        assert_eq!(expected.request.params, params);
        (id, request)
    }

    fn send(&self, id: RequestId, request: Call) -> Self::Out {
        self.expected_request_responses
            .borrow_mut()
            .pop_front()
            .map(|expected| future::ok(expected.response))
            .ok_or_else(|| format!("unexpected request (id: {:?}): {:?}", id, request))
            .unwrap()
    }
}

impl TestTransport {
    /// Create a new test transport instance.
    pub fn new() -> Self {
        Default::default()
    }

    /// Add an expected request and response that will be sent.
    pub fn add_request_response(&mut self, method: String, params: Vec<Value>, response: Value) {
        self.expected_request_responses
            .borrow_mut()
            .push_back(RequestResponse {
                request: Request { method, params },
                response,
            });
    }

    pub fn assert_no_more_requests(&self) {
        assert!(self.expected_request_responses.borrow().is_empty());
    }
}
