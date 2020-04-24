//! This module implements event builders and streams for retrieving events
//! emitted by a contract.

use crate::errors::ExecutionError;
use futures::compat::{Compat01As03, Future01CompatExt, Stream01CompatExt};
use futures::ready;
use futures::stream::Stream;
use pin_project::{pin_project, project};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use web3::api::{CreateFilter, FilterStream, Web3};
use web3::types::{Filter, Log};
use web3::Transport;

/// A log stream that emits logs matching a certain filter.
///
/// Note that when creating a log stream that is only valid until a certain
/// block number, the `Stream` implementation will currently remain in the
/// pending state indefinitely.
#[must_use = "streams do nothing unless you poll them"]
#[pin_project]
pub struct LogStream<T: Transport> {
    #[pin]
    state: LogStreamState<T>,
}

/// The state of the log stream. It can either be creating a new log filter for
/// retrieving new logs or streaming logs from the created filter.
#[pin_project]
enum LogStreamState<T: Transport> {
    CreatingFilter(#[pin] CompatCreateFilter<T, Log>, Duration),
    Streaming(#[pin] CompatFilterStream<T, Log>),
}

impl<T: Transport> LogStream<T> {
    /// Create a new log stream from a given web3 provider, filter and polling
    /// parameters.
    pub fn new(web3: Web3<T>, filter: Filter, poll_interval: Duration) -> Self {
        let create_filter = web3.eth_filter().create_logs_filter(filter).compat();
        let state = LogStreamState::CreatingFilter(create_filter, poll_interval);
        LogStream { state }
    }
}

impl<T: Transport> Stream for LogStream<T> {
    type Item = Result<Log, ExecutionError>;

    #[project]
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        loop {
            let mut state = self.as_mut().project().state;

            #[project]
            let next_state = match state.as_mut().project() {
                LogStreamState::CreatingFilter(create_filter, poll_interval) => {
                    let log_filter = match ready!(create_filter.poll(cx)) {
                        Ok(log_filter) => log_filter,
                        Err(err) => return Poll::Ready(Some(Err(err.into()))),
                    };
                    let stream = log_filter.stream(*poll_interval).compat();
                    LogStreamState::Streaming(stream)
                }
                LogStreamState::Streaming(stream) => {
                    return stream
                        .poll_next(cx)
                        .map(|result| result.map(|log| Ok(log?)))
                }
            };

            *state = next_state;
        }
    }
}

/// A type alias for a stream that emits logs.
type CompatFilterStream<T, R> = Compat01As03<FilterStream<T, R>>;

/// A type alias for a future that resolves with the ID of a created log filter
/// that can be queried in order to stream logs.
type CompatCreateFilter<T, R> = Compat01As03<CreateFilter<T, R>>;

/// A log filter.
///
/// Note this type is similar to `web3::types::Filter` with the notable
/// exception that it has public access to fields so that they can be inspected
/// and used during streaming. In fact, they are currently needed for spliting
/// the queried block range into pages to allow for 
pub struct LogFilter {
    /// The block to start streaming logs from.
    pub from_block: Option<BlockNumber>,
    /// The block to stop streaming logs from.
    pub to_block: Option<BlockNumber>,
    /// The contract addresses to filter logs for.
    pub address: Vec<Address>,
    /// Topic filters used for filtering logs based on indexed topics.
    pub topics: TopicFilter,
}

/// A log stream that emits logs matching a certain filter.
///
/// Note that when creating a log stream that is only valid until a certain
/// block number, the `Stream` implementation will currently remain in the
/// pending state indefinitely.
#[must_use = "streams do nothing unless you poll them"]
#[pin_project]
pub struct LogStream2<T: Transport> {
    #[pin]
    state: LogStreamState<T>,
}

/// The state of the log stream. It can either be creating a new log filter for
/// retrieving new logs or streaming logs from the created filter.
#[pin_project]
enum LogStreamState<T: Transport> {
    CreatingFilter(#[pin] CompatCreateFilter<T, Log>, Duration),
    Streaming(#[pin] CompatFilterStream<T, Log>),
}

impl<T: Transport> LogStream<T> {
    /// Create a new log stream from a given web3 provider, filter and polling
    /// parameters.
    pub fn new(web3: Web3<T>, filter: Filter, poll_interval: Duration) -> Self {
        let create_filter = web3.eth_filter().create_logs_filter(filter).compat();
        let state = LogStreamState::CreatingFilter(create_filter, poll_interval);
        LogStream { state }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::prelude::*;
    use futures::stream::StreamExt;
    use serde_json::Value;
    use web3::types::{Address, H256};

    fn generate_log(kind: &str) -> Value {
        json!({
            "address": Address::zero(),
            "topics": [],
            "data": "0x",
            "blockHash": H256::zero(),
            "blockNumber": "0x0",
            "transactionHash": H256::zero(),
            "transactionIndex": "0x0",
            "logIndex": "0x0",
            "transactionLogIndex": "0x0",
            "logType": kind,
            "removed": false,
        })
    }

    #[test]
    fn log_stream_next_log() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        // filter created
        transport.add_response(json!("0xf0"));
        // get logs filter
        transport.add_response(json!([generate_log("awesome")]));

        let log = LogStream::new(web3, Default::default(), Duration::from_secs(0))
            .next()
            .immediate()
            .expect("log stream did not produce any logs")
            .expect("failed to get log from log stream");

        assert_eq!(log.log_type.as_deref(), Some("awesome"));
        transport.assert_request("eth_newFilter", &[json!({})]);
        transport.assert_request("eth_getFilterChanges", &[json!("0xf0")]);
        transport.assert_no_more_requests();
    }
}
