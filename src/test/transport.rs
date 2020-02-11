//! Implementation of a transport for testing purposes.

use jsonrpc_core::{Call, Value};
use std::cell::RefCell;
use std::clone::Clone;
use std::fmt::Debug;
use std::rc::Rc;
use web3::error::Error;
use web3::futures::future::FutureResult;
use web3::{RequestId, Transport};

mockall::mock! {
    pub Transport {
        fn execute(&self, method: &str, params: Vec<Value>) -> FutureResult<Value, Error>;
    }
}

// `Debug` is needed so that `TestTransport` can derive `Debug`.
impl Debug for MockTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MockTransport")
    }
}

/// Implements `Transport` forwarding calls to `execute` to a mockable struct.
///
/// `MockTransport` itself does not implement `Transport` because we only use
/// `execute` because it is the most convenient function to mock.
#[derive(Debug, Clone)]
pub struct TestTransport_ {
    mock_transport: Rc<RefCell<MockTransport>>,
}

impl TestTransport_ {
    pub fn new() -> TestTransport_ {
        TestTransport_ {
            mock_transport: Rc::new(RefCell::new(MockTransport::new())),
        }
    }

    /// Access the underlying struct on which `expect_execute` can be called.
    ///
    /// Note that no instance of the `RefMut` can be alive when the transport
    /// gets called because it represents a mutable reference.
    pub fn mock(&self) -> std::cell::RefMut<MockTransport> {
        self.mock_transport.as_ref().borrow_mut()
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
        self.mock_transport.borrow().execute(method, params)
    }
}

// ----------------------------------------------------

use std::collections::VecDeque;
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
