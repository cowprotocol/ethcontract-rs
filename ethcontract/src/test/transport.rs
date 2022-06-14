//! Implementation of a transport for testing purposes. This is largely based on
//! the `rust-web3` `TestTransport` type with some modifications.

use jsonrpc_core::{Call, Value};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use web3::futures::future::{self, Ready};
use web3::helpers;
use web3::{error::Error, BatchTransport};
use web3::{RequestId, Transport};

/// Type alias for request method and value pairs
type Requests = Vec<(String, Vec<Value>)>;

#[derive(Debug, Default)]
struct Inner {
    asserted: usize,
    requests: Requests,
    responses: VecDeque<Value>,
}

/// Test transport
#[derive(Debug, Default, Clone)]
pub struct TestTransport {
    inner: Arc<Mutex<Inner>>,
}

impl Transport for TestTransport {
    type Out = Ready<Result<Value, Error>>;

    fn prepare(&self, method: &str, params: Vec<Value>) -> (RequestId, Call) {
        let request = helpers::build_request(1, method, params.clone());
        let mut inner = self.inner.lock().unwrap();
        inner.requests.push((method.into(), params));
        (inner.requests.len(), request)
    }

    fn send(&self, id: RequestId, request: Call) -> Self::Out {
        let response = self.inner.lock().unwrap().responses.pop_front();
        match response {
            Some(response) => future::ok(response),
            None => {
                println!("Unexpected request (id: {:?}): {:?}", id, request);
                future::err(Error::Unreachable)
            }
        }
    }
}

impl BatchTransport for TestTransport {
    type Batch = Ready<Result<Vec<Result<Value, Error>>, Error>>;

    fn send_batch<T>(&self, requests: T) -> Self::Batch
    where
        T: IntoIterator<Item = (RequestId, Call)>,
    {
        let mut requests: Vec<_> = requests.into_iter().collect();

        // Only send the first request to receive a response for all requests in the batch
        let (id, call) = match requests.pop() {
            Some(request) => request,
            None => return future::err(Error::Unreachable),
        };

        let responses = match self
            .send(id, call)
            .into_inner()
            .ok()
            .and_then(|value| value.as_array().cloned())
        {
            Some(array) => array.into_iter(),
            None => {
                println!("Response should return a list of values");
                return future::err(Error::Unreachable);
            }
        };
        future::ok(responses.map(Ok).collect())
    }
}

impl TestTransport {
    /// Create a new test transport instance.
    pub fn new() -> Self {
        Default::default()
    }

    /// Add a response to an eventual request.
    pub fn add_response(&mut self, value: Value) {
        let mut inner = self.inner.lock().unwrap();
        inner.responses.push_back(value);
    }

    /// Assert that a request was made.
    pub fn assert_request(&mut self, method: &str, params: &[Value]) {
        let mut inner = self.inner.lock().unwrap();
        let idx = inner.asserted;
        inner.asserted += 1;

        let (m, p) = inner.requests.get(idx).expect("Expected result.").clone();
        assert_eq!(&m, method);
        assert_eq!(&p[..], params);
    }

    /// Assert that there are no more pending requests.
    pub fn assert_no_more_requests(&self) {
        let inner = self.inner.lock().unwrap();
        assert_eq!(
            inner.asserted,
            inner.requests.len(),
            "Expected no more requests, got: {:?}",
            &inner.requests[inner.asserted..]
        );
    }
}
