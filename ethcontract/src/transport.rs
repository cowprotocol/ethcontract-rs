//! Module containing additional transports used by the `ethcontract` runtime. In
//! particular, this module includes `DynTransport` which wraps other valid
//! transports and using dynamic dispatch to call the underlying transport
//! implementation. This transport is used by default for generated contract APIs
//! to help create a more ergonimic experience by making the generated struct not
//! be generic on the underlying transport (at the small cost of some dynamic
//! dispatch and extra allocations).

use futures::future::BoxFuture;
use futures::FutureExt as _;
use jsonrpc_core::Call;
use serde_json::Value;
use std::any::Any;
use std::fmt::Debug;
use std::future::Future;
use std::sync::Arc;
use web3::error::Error as Web3Error;
use web3::{RequestId, Transport};

/// Type alias for the output future in for the `DynTransport`'s `Transport`
/// implementation.
type BoxedFuture = BoxFuture<'static, Result<Value, Web3Error>>;

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
    F: Future<Output = Result<Value, Web3Error>> + Send + 'static,
    T: Transport<Out = F>,
{
    #[inline(always)]
    fn prepare_boxed(&self, method: &str, params: Vec<Value>) -> (RequestId, Call) {
        self.prepare(method, params)
    }

    #[inline(always)]
    fn send_boxed(&self, id: RequestId, request: Call) -> BoxedFuture {
        self.send(id, request).boxed()
    }

    #[inline(always)]
    fn execute_boxed(&self, method: &str, params: Vec<Value>) -> BoxedFuture {
        self.execute(method, params).boxed()
    }
}

/// Dynamic `Transport` implementation to allow for a generic-free contract API.
/// This type wraps any `Transport` type and implements `Transport` itself.
#[derive(Debug)]
pub struct DynTransport {
    inner: Arc<dyn TransportBoxed + Sync + Send + 'static>,
}

impl DynTransport {
    /// Wrap a `Transport` in a `DynTransport`
    pub fn new<F, T>(inner: T) -> Self
    where
        F: Future<Output = Result<Value, Web3Error>> + Send + 'static,
        T: Transport<Out = F> + Sync + Send + 'static,
    {
        let inner_ref: &dyn Any = &inner;
        let inner_arc = match inner_ref.downcast_ref::<DynTransport>() {
            // NOTE: If a `DynTransport` is being created from another
            //   `DynTransport`, then just clone its inner transport instead of
            //   re-wrapping it.
            Some(dyn_transport) => dyn_transport.inner.clone(),
            None => Arc::new(inner),
        };

        DynTransport { inner: inner_arc }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::prelude::*;

    #[test]
    fn dyn_transport() {
        let mut transport = TestTransport::new();
        let dyn_transport = DynTransport::new(transport.clone());

        // assert that the underlying transport prepares the request.
        let (id, call) = dyn_transport.prepare("test", vec![json!(28)]);
        transport.assert_request("test", &[json!(28)]);
        transport.assert_no_more_requests();

        // assert that the underlying transport returns the response.
        transport.add_response(json!(true));
        let response = dyn_transport.send(id, call).immediate().expect("success");
        assert_eq!(response, json!(true));

        // assert that the transport layer gets propagated - it errors here since
        // we did not provide the test transport with a response
        dyn_transport
            .execute("test", vec![json!(42)])
            .immediate()
            .err()
            .expect("failed");
        transport.assert_request("test", &[json!(42)]);
        transport.assert_no_more_requests();
    }

    #[test]
    #[allow(clippy::redundant_clone)]
    fn dyn_transport_does_not_double_wrap() {
        let transport = TestTransport::new();
        let dyn_transport = DynTransport::new(transport);
        assert_eq!(Arc::strong_count(&dyn_transport.inner), 1);

        let dyn_dyn_transport = DynTransport::new(dyn_transport.clone());
        // NOTE: We expect the newly created `dyn_dyn_transport` to have two
        //   references to its inner transport, one held by itself, and the
        //   other by `dyn_transport`. If the `dyn_transport` was being
        //   re-wrapped in an `Arc`, then the count would only be 1.
        assert_eq!(Arc::strong_count(&dyn_dyn_transport.inner), 2);
    }

    #[test]
    fn dyn_transport_is_threadsafe() {
        let transport = TestTransport::new();
        let dyn_transport = DynTransport::new(transport);
        std::thread::spawn(move || {
            // Try to use the transport in the thread which would fail to
            // if it wasn't safe.
            let _ = dyn_transport.prepare("test", vec![json!(28)]);
        });
    }
}
