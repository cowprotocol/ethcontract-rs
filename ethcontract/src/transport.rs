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
use web3::{BatchTransport, RequestId, Transport};

/// Type alias for the output future in for the `DynTransport`'s implementation.
type BoxedFuture = BoxFuture<'static, Result<Value, Web3Error>>;

/// Helper trait that wraps `Transport` trait so it can be used as a trait
/// object. This trait is implemented for all `Transport`'s.
trait TransportBoxed: Debug + Sync + Send + 'static {
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
    T: Transport<Out = F> + Sync + Send + 'static,
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
#[derive(Debug, Clone)]
pub struct DynTransport {
    inner: Arc<dyn TransportBoxed>,
}

impl DynTransport {
    /// Wrap a `Transport` in a `DynTransport`
    pub fn new<F, T>(inner: T) -> Self
    where
        F: Future<Output = Result<Value, Web3Error>> + Send + 'static,
        T: Transport<Out = F> + Sync + Send + 'static,
    {
        let inner_ref: &dyn Any = &inner;

        // NOTE: If a `DynTransport` is being created from another `DynTransport`,
        //       or from `DynBatchTransport`, then just clone its inner transport
        //       instead of re-wrapping it.
        let inner_arc = if let Some(dyn_transport) = inner_ref.downcast_ref::<DynTransport>() {
            dyn_transport.inner.clone()
        } else if let Some(dyn_batch_transport) = inner_ref.downcast_ref::<DynBatchTransport>() {
            dyn_batch_transport.inner.clone().into_transport()
        } else {
            Arc::new(inner)
        };

        DynTransport { inner: inner_arc }
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

/// Type alias for the output future in for the `BatchDynTransport`'s implementation.
type BoxedBatch = BoxFuture<'static, Result<Vec<Result<Value, Web3Error>>, Web3Error>>;

/// Helper trait that wraps `BatchTransport` trait so it can be used as a trait
/// object. This trait is implemented for all `BatchTransport`'s.
trait BatchTransportBoxed: TransportBoxed {
    /// Wraps `BatchTransport::send_batch`
    fn send_batch_boxed(&self, requests: Vec<(RequestId, Call)>) -> BoxedBatch;

    fn into_transport(self: Arc<Self>) -> Arc<dyn TransportBoxed>;
}

impl<B, T> BatchTransportBoxed for T
where
    B: Future<Output = Result<Vec<Result<Value, Web3Error>>, Web3Error>> + Send + 'static,
    T: TransportBoxed + BatchTransport<Batch = B>,
{
    #[inline(always)]
    fn send_batch_boxed(&self, requests: Vec<(RequestId, Call)>) -> BoxedBatch {
        self.send_batch(requests.into_iter()).boxed()
    }

    #[inline(always)]
    fn into_transport(self: Arc<Self>) -> Arc<dyn TransportBoxed> {
        self
    }
}

/// Dynamic `BatchTransport` implementation to allow for a generic-free contract API.
/// This type wraps any `BatchTransport` type and implements `BatchTransport` itself.
#[derive(Debug, Clone)]
pub struct DynBatchTransport {
    inner: Arc<dyn BatchTransportBoxed>,
}

impl DynBatchTransport {
    /// Wrap a `Transport` in a `DynBatchTransport`
    pub fn new<F, B, T>(inner: T) -> Self
    where
        F: Future<Output = Result<Value, Web3Error>> + Send + 'static,
        B: Future<Output = Result<Vec<Result<Value, Web3Error>>, Web3Error>> + Send + 'static,
        T: Transport<Out = F> + BatchTransport<Batch = B> + Sync + Send + 'static,
    {
        let inner_ref: &dyn Any = &inner;

        // NOTE: If a `DynTransport` is being created from another `DynTransport`,
        //       or from `DynBatchTransport`, then just clone its inner transport
        //       instead of re-wrapping it.
        let inner_arc =
            if let Some(dyn_batch_transport) = inner_ref.downcast_ref::<DynBatchTransport>() {
                dyn_batch_transport.inner.clone()
            } else {
                Arc::new(inner)
            };

        DynBatchTransport { inner: inner_arc }
    }
}

impl Transport for DynBatchTransport {
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

impl BatchTransport for DynBatchTransport {
    type Batch = BoxedBatch;

    #[inline(always)]
    fn send_batch<T>(&self, requests: T) -> Self::Batch
    where
        T: IntoIterator<Item = (RequestId, Call)>,
    {
        self.inner.send_batch_boxed(requests.into_iter().collect())
    }
}

impl From<DynBatchTransport> for DynTransport {
    fn from(t: DynBatchTransport) -> Self {
        DynTransport {
            inner: t.inner.into_transport(),
        }
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
