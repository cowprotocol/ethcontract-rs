//! Module containing additional transports used by the `ethcontract` runtime. In
//! particular, this module includes `DynTransport` which wraps other valid
//! transports and using dynamic dispatch to call the underlying transport
//! implementation. This transport is used by default for generated contract APIs
//! to help create a more ergonimic experience by making the generated struct not
//! be generic on the underlying transport (at the small cost of some dynamic
//! dispatch and extra allocations).

use jsonrpc_core::Call;
use serde_json::Value;
use std::fmt::Debug;
use std::sync::Arc;
use web3::error::Error as Web3Error;
use web3::futures::Future;
use web3::{RequestId, Transport};

/// Type alias for the output future in for the `DynTransport`'s `Transport`
/// implementation.
type BoxedFuture = Box<dyn Future<Item = Value, Error = Web3Error> + Send + 'static>;

/// Helper trait that wraps `Transport` trait so it can be used as a trait
/// object. This trait is implemented for all `Transport`'s.
trait TransportBoxed: Debug {
    /// Wraps `Transport::prepend`
    fn prepare_boxed(&self, method: &str, params: Vec<Value>) -> (RequestId, Call);
    /// Wraps `Transport::send`
    fn send_boxed(&self, id: RequestId, request: Call) -> BoxedFuture;
    /// Wraps `Transport::execute`
    fn execute_boxed(&self, method: &str, params: Vec<Value>) -> BoxedFuture;
}

impl<F, T> TransportBoxed for T
where
    F: Future<Item = Value, Error = Web3Error> + Send + 'static,
    T: Transport<Out = F>,
{
    #[inline(always)]
    fn prepare_boxed(&self, method: &str, params: Vec<Value>) -> (RequestId, Call) {
        self.prepare(method, params)
    }

    #[inline(always)]
    fn send_boxed(&self, id: RequestId, request: Call) -> BoxedFuture {
        Box::new(self.send(id, request))
    }

    #[inline(always)]
    fn execute_boxed(&self, method: &str, params: Vec<Value>) -> BoxedFuture {
        Box::new(self.execute(method, params))
    }
}

/// Dynamic `Transport` implementation to allow for a generic-free contract API.
/// This type wraps any `Transport` type and implements `Transport` itself.
#[derive(Debug)]
pub struct DynTransport {
    inner: Arc<dyn TransportBoxed + 'static>,
}

impl DynTransport {
    /// Wrap a `Transport` in a `DynTransport`
    pub fn new<F, T>(inner: T) -> DynTransport
    where
        F: Future<Item = Value, Error = Web3Error> + Send + 'static,
        T: Transport<Out = F> + 'static,
    {
        DynTransport {
            inner: Arc::new(inner),
        }
    }
}

impl Clone for DynTransport {
    fn clone(&self) -> Self {
        DynTransport {
            inner: self.inner.clone(),
        }
    }
}

impl Transport for DynTransport {
    type Out = BoxedFuture;

    #[inline(always)]
    fn prepare(&self, method: &str, params: Vec<Value>) -> (RequestId, Call) {
        self.inner.prepare_boxed(method, params)
    }

    #[inline(always)]
    fn send(&self, id: RequestId, request: Call) -> Self::Out {
        self.inner.send_boxed(id, request)
    }

    #[inline(always)]
    fn execute(&self, method: &str, params: Vec<Value>) -> Self::Out {
        self.inner.execute_boxed(method, params)
    }
}
