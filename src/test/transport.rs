//! Implementation of a transport for testing purposes.

use jsonrpc_core::{Call, Value};
use std::cell::RefCell;
use std::clone::Clone;
use std::collections::VecDeque;
use std::fmt::Debug;
use std::rc::Rc;
use web3::error::Error;
use web3::futures::future::FutureResult;
use web3::{RequestId, Transport};

// ----------------------------------------------------

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
    response: Result<Value, Error>,
}

#[derive(Debug, Clone, Default)]
pub struct TestTransport_ {
    request_responses: Rc<RefCell<VecDeque<RequestResponse>>>,
}

impl TestTransport_ {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn expect_request(
        &mut self,
        method: impl Into<String>,
        params: impl AsRef<[Value]>,
        response: Result<Value, Error>,
    ) {
        self.request_responses
            .borrow_mut()
            .push_back(RequestResponse {
                request: Request {
                    method: method.into(),
                    params: params.as_ref().to_vec(),
                },
                response,
            });
    }

    pub fn assert_no_missing_requests(&self) {
        if let Some(request_response) = self.request_responses.borrow().front() {
            panic!(
                "transport dropped without observing all expected requests: \
                 first missing request is {}",
                request_response.request.method
            );
        }
    }
}

impl Transport for TestTransport_ {
    type Out = FutureResult<Value, Error>;

    fn prepare(&self, _method: &str, _params: Vec<Value>) -> (RequestId, Call) {
        unimplemented!();
    }

    fn send(&self, _id: RequestId, _request: Call) -> Self::Out {
        unimplemented!();
    }

    fn execute(&self, method: &str, params: Vec<Value>) -> Self::Out {
        let RequestResponse { request, response } = self
            .request_responses
            .borrow_mut()
            .pop_front()
            .ok_or_else(|| {
                format!(
                    "unexpected request with method {} and params {:?}",
                    method, params
                )
            })
            .unwrap();
        assert_eq!(request.method, method);
        assert_eq!(request.params, params);
        web3::futures::future::result(response)
    }
}

// ----------------------------------------------------

use web3::helpers;

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
            Some(response) => web3::futures::future::ok(response),
            None => {
                println!("Unexpected request (id: {:?}): {:?}", id, request);
                web3::futures::future::err(Error::Unreachable)
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
