//! Module implements type-safe event streams from an ABI event definition with
//! detokenization of the data included in the log.

use crate::abicompat::AbiCompat;
use crate::errors::{EventError, ExecutionError};
use crate::log::LogStream;
pub use ethcontract_common::abi::Topic;
use ethcontract_common::abi::{Event as AbiEvent, RawLog, RawTopicFilter, Token};
use futures::stream::Stream;
use pin_project::{pin_project, project};
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use web3::api::Web3;
use web3::contract::tokens::{Detokenize, Tokenizable};
use web3::types::{Address, BlockNumber, FilterBuilder};
use web3::Transport;

/// A type representing a contract event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Event<T> {
    /// A new event was received.
    Added(T),
    /// A previously revent was removed as the result of a re-org.
    Removed(T),
}

impl<T> Event<T> {
    /// Get the underlying event data regardless of whether the event was added
    /// or removed.
    pub fn into_data(self) -> T {
        match self {
            Event::Added(value) => value,
            Event::Removed(value) => value,
        }
    }

    /// Get a reference the underlying event data regardless of whether the
    /// event was added or removed.
    pub fn as_data(&self) -> &T {
        match self {
            Event::Added(value) => value,
            Event::Removed(value) => value,
        }
    }

    /// Get a mutable reference the underlying event data regardless of whether
    /// the event was added or removed.
    pub fn as_data_mut(&mut self) -> &mut T {
        match self {
            Event::Added(value) => value,
            Event::Removed(value) => value,
        }
    }

    /// Get the underlying event data if the event was added, `None` otherwise.
    pub fn added(self) -> Option<T> {
        match self {
            Event::Added(value) => Some(value),
            Event::Removed(_) => None,
        }
    }

    /// Get the underlying event data if the event was removed, `None`
    /// otherwise.
    pub fn removed(self) -> Option<T> {
        match self {
            Event::Removed(value) => Some(value),
            Event::Added(_) => None,
        }
    }
}

/// The default poll interval to use for polling logs from the block chain.
#[cfg(not(test))]
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5);
#[cfg(test)]
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(0);

/// A builder for creating a filtered stream of contract events that are
#[must_use = "event builders do nothing unless you stream them"]
pub struct EventBuilder<T: Transport, E: Detokenize> {
    /// The underlying web3 instance.
    web3: Web3<T>,
    /// The event ABI data for encoding topic filters and decoding logs.
    event: AbiEvent,
    /// The web3 filter builder used for creating a log filter.
    filter: FilterBuilder,
    /// The topic filters that are encoded based on the event ABI.
    pub topics: RawTopicFilter,
    /// The polling interval for querying the node for more events.
    pub poll_interval: Option<Duration>,
    _event: PhantomData<E>,
}

impl<T: Transport, E: Detokenize> EventBuilder<T, E> {
    /// Creates a new event builder from a web3 provider and a contract event
    /// and address.
    pub fn new(web3: Web3<T>, event: AbiEvent, address: Address) -> Self {
        EventBuilder {
            web3,
            event,
            filter: FilterBuilder::default().address(vec![address]),
            topics: RawTopicFilter::default(),
            poll_interval: None,
            _event: PhantomData,
        }
    }

    /// Sets the starting block from which to stream logs for.
    ///
    /// If left unset defaults to the latest block.
    #[allow(clippy::wrong_self_convention)]
    pub fn from_block(mut self, block: BlockNumber) -> Self {
        self.filter = self.filter.from_block(block);
        self
    }

    /// Sets the last block from which to stream logs for.
    ///
    /// If left unset defaults to the streaming until the end of days.
    #[allow(clippy::wrong_self_convention)]
    pub fn to_block(mut self, block: BlockNumber) -> Self {
        self.filter = self.filter.to_block(block);
        self
    }

    /// Adds a filter for the first indexed topic.
    ///
    /// This corresponds to the first indexed property, which for anonymous
    /// events corresponds to `topic[0]` in the log, and for named events is
    /// actually `topic[1]`.
    pub fn topic0<P>(mut self, topic: Topic<P>) -> Self
    where
        P: Tokenizable,
    {
        self.topics.topic0 = tokenize_topic(topic);
        self
    }

    /// Adds a filter for the second indexed topic.
    pub fn topic1<P>(mut self, topic: Topic<P>) -> Self
    where
        P: Tokenizable,
    {
        self.topics.topic1 = tokenize_topic(topic);
        self
    }

    /// Adds a filter for the third indexed topic.
    pub fn topic2<P>(mut self, topic: Topic<P>) -> Self
    where
        P: Tokenizable,
    {
        self.topics.topic2 = tokenize_topic(topic);
        self
    }

    /// The polling interval. This is used as the interval between consecutive
    /// `eth_getFilterChanges` calls to get filter updates.
    pub fn poll_interval(mut self, value: Duration) -> Self {
        self.poll_interval = Some(value);
        self
    }

    /// Creates an event stream from the current event builder.
    pub fn stream(self) -> Result<EventStream<T, E>, EventError> {
        EventStream::from_builder(self)
    }
}

/// Converts a tokenizable topic into a raw topic for filtering.
fn tokenize_topic<P>(topic: Topic<P>) -> Topic<Token>
where
    P: Tokenizable,
{
    topic.map(|parameter| parameter.into_token().compat())
}

/// An event stream that emits events matching a builder.
#[must_use = "streams do nothing unless you or poll them"]
#[pin_project]
pub struct EventStream<T: Transport, E: Detokenize> {
    event: AbiEvent,
    #[pin]
    inner: LogStream<T>,
    _event: PhantomData<E>,
}

impl<T: Transport, E: Detokenize> EventStream<T, E> {
    /// Create a new log stream from a given web3 provider, filter and polling
    /// parameters.
    pub fn from_builder(builder: EventBuilder<T, E>) -> Result<Self, EventError> {
        let event = builder.event;

        let web3 = builder.web3;
        let filter = {
            let abi_filter = event
                .filter(builder.topics)
                .map_err(|err| EventError::new(&event, err))?;
            builder.filter.topic_filter(abi_filter.compat()).build()
        };

        let poll_interval = builder.poll_interval.unwrap_or(DEFAULT_POLL_INTERVAL);

        let inner = LogStream::new(web3, filter, poll_interval);

        Ok(EventStream {
            event,
            inner,
            _event: PhantomData,
        })
    }
}

impl<T: Transport, E: Detokenize> Stream for EventStream<T, E> {
    type Item = Result<Event<E>, EventError>;

    #[project]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        #[project]
        let EventStream { event, inner, .. } = self.project();
        inner.poll_next(cx).map(|next| {
            next.map(|log| {
                let log = log?;
                let event_log = event.parse_log(RawLog {
                    topics: log.topics,
                    data: log.data.0,
                })?;
                let tokens = event_log
                    .params
                    .into_iter()
                    .map(|param| param.value)
                    .collect::<Vec<_>>()
                    .compat()
                    .ok_or(ExecutionError::UnsupportedToken)?;
                let event_data = E::from_tokens(tokens)?;

                Ok(if log.removed == Some(true) {
                    Event::Removed(event_data)
                } else {
                    Event::Added(event_data)
                })
            })
            .map(|next: Result<_, ExecutionError>| next.map_err(|err| EventError::new(&event, err)))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::prelude::*;
    use ethcontract_common::abi::{EventParam, ParamType};
    use futures::stream::StreamExt;
    use serde_json::Value;
    use web3::types::{Address, H256, U256};

    fn test_abi_event() -> (AbiEvent, Value) {
        let event = AbiEvent {
            name: "test".to_owned(),
            inputs: vec![
                EventParam {
                    name: "from".to_owned(),
                    kind: ParamType::Address,
                    indexed: true,
                },
                EventParam {
                    name: "to".to_owned(),
                    kind: ParamType::Address,
                    indexed: true,
                },
                EventParam {
                    name: "amount".to_owned(),
                    kind: ParamType::Uint(256),
                    indexed: false,
                },
            ],
            anonymous: false,
        };
        let log = json!({
            "address": Address::zero(),
            "topics": [
                event.signature(),
                H256::from(Address::repeat_byte(0xf0)),
                H256::from(Address::repeat_byte(0x70)),
            ],
            "data": H256::from_low_u64_be(42),
            "blockHash": H256::zero(),
            "blockNumber": "0x0",
            "transactionHash": H256::zero(),
            "transactionIndex": "0x0",
            "logIndex": "0x0",
            "transactionLogIndex": "0x0",
            "logType": "",
            "removed": false,
        });

        (event, log)
    }

    #[test]
    fn event_stream_next_event() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());
        let (event, log) = test_abi_event();

        // filter created
        transport.add_response(json!("0xf0"));
        // get logs filter
        transport.add_response(json!([log]));

        let address = Address::repeat_byte(0x01);
        let signature = event.signature();
        let (_, _, amount) = EventBuilder::<_, (Address, Address, U256)>::new(web3, event, address)
            .to_block(99.into())
            .topic1(Topic::OneOf(vec![
                Address::repeat_byte(0x70),
                Address::repeat_byte(0x80),
            ]))
            .stream()
            .expect("failed to abi-encode filter")
            .next()
            .immediate()
            .expect("log stream did not produce any logs")
            .expect("failed to get log from log stream")
            .added()
            .expect("expected an added event");

        assert_eq!(amount, U256::from(42));
        transport.assert_request(
            "eth_newFilter",
            &[json!({
                "address": address,
                "toBlock": U256::from(99),
                "topics": [
                    signature,
                    null,
                    [
                        H256::from(Address::repeat_byte(0x70)),
                        H256::from(Address::repeat_byte(0x80)),
                    ],
                ],
            })],
        );
        transport.assert_request("eth_getFilterChanges", &[json!("0xf0")]);
        transport.assert_no_more_requests();
    }
}
