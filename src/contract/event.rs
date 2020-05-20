//! Module implements type-safe event streams from an ABI event definition with
//! detokenization of the data included in the log.

mod data;

pub use self::data::{Event, EventData, EventMetadata, ParseLog, RawLog};
use crate::errors::{EventError, ExecutionError};
use crate::future::CompatCallFuture;
use crate::log::LogStream;
pub use ethcontract_common::abi::Topic;
use ethcontract_common::abi::{Event as AbiEvent, RawTopicFilter, Token, TopicFilter};
use futures::compat::Future01CompatExt;
use futures::stream::Stream;
use pin_project::{pin_project, project};
use std::cmp;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use web3::api::Web3;
use web3::contract::tokens::{Detokenize, Tokenizable};
use web3::types::{Address, BlockNumber, FilterBuilder, Log, H256};
use web3::Transport;

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

    /// Limit the number of events that can be retrieved by this filter.
    ///
    /// Note that this parameter is non-standard.
    pub fn limit(mut self, value: usize) -> Self {
        self.filter = self.filter.limit(value);
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

    /// Returns a future that resolves with a collection of all existing logs
    /// matching the builder parameters.
    pub fn query(self) -> Result<QueryFuture<T, E>, EventError> {
        QueryFuture::from_builder(self)
    }

    /// Creates an event stream from the current event builder that emits new
    /// events.
    pub fn stream(self) -> Result<EventStream<T, E>, EventError> {
        EventStream::from_builder(self)
    }
}

/// Converts a tokenizable topic into a raw topic for filtering.
fn tokenize_topic<P>(topic: Topic<P>) -> Topic<Token>
where
    P: Tokenizable,
{
    topic.map(|parameter| parameter.into_token())
}

/// A future for querying events based on a log filter.
#[must_use = "futures do nothing unless you await or poll them"]
#[pin_project]
pub struct QueryFuture<T: Transport, E: Detokenize> {
    event: AbiEvent,
    #[pin]
    inner: CompatCallFuture<T, Vec<Log>>,
    _event: PhantomData<E>,
}

impl<T: Transport, E: Detokenize> QueryFuture<T, E> {
    /// Create a new query future from event builder parameters.
    pub fn from_builder(builder: EventBuilder<T, E>) -> Result<Self, EventError> {
        let event = builder.event;

        let web3 = builder.web3;
        let filter = {
            let abi_filter = event
                .filter(builder.topics)
                .map_err(|err| EventError::new(&event, err))?;
            builder.filter.topic_filter(abi_filter).build()
        };

        let inner = web3.eth().logs(filter).compat();

        Ok(QueryFuture {
            event,
            inner,
            _event: PhantomData,
        })
    }
}

impl<T: Transport, E: Detokenize> Future for QueryFuture<T, E> {
    type Output = Result<Vec<Event<E>>, EventError>;

    #[project]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        #[project]
        let QueryFuture { event, inner, .. } = self.project();

        inner
            .poll(cx)
            .map(|logs| {
                logs?
                    .into_iter()
                    .map(|log| Event::from_log(log, |raw| raw.decode(event)))
                    .collect::<Result<Vec<_>, ExecutionError>>()
            })
            .map(|result| result.map_err(|err| EventError::new(&event, err)))
    }
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
            builder.filter.topic_filter(abi_filter).build()
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
            next.map(|log| Event::from_log(log?, |raw| raw.decode(event)))
                .map(|next| next.map_err(|err| EventError::new(&event, err)))
        })
    }
}

/// The default block page size used for querying past events.
pub const DEFAULT_BLOCK_PAGE_SIZE: u64 = 10_000;

/// A builder for creating a filtered stream for any contract event.
#[must_use = "event builders do nothing unless you stream them"]
pub struct AllEventsBuilder<T: Transport, E: ParseLog> {
    /// The underlying web3 instance.
    web3: Web3<T>,
    /// The web3 filter builder used for creating a log filter.
    filter: FilterBuilder,
    /// The block to start retrieving logs from.
    ///
    /// This needs to be stored to work around the fact that the web3 filter
    /// does not allow access to these values once stored.
    pub from_block: Option<BlockNumber>,
    /// The last block to retrieve logs for.
    pub to_block: Option<BlockNumber>,
    /// The topic filters that are encoded based on the event ABI.
    pub topics: TopicFilter,
    /// The polling interval for querying the node for more events.
    pub poll_interval: Option<Duration>,
    /// The contract deployment transaction hash. Specifying this can increase
    /// the performance of the paginated events query.
    ///
    /// Note that if the contract was created from an existing deployment that
    /// includes the transaction hash, then this property will be automatically
    /// set.
    pub deployment_transaction: Option<H256>,
    /// The page size in blocks to use when doing a paginated query on past
    /// events. This provides no guarantee in how many events will be returned
    /// per page, but used to limit the block range for the query.
    pub block_page_size: Option<u64>,
    _events: PhantomData<E>,
}

impl<T: Transport, E: ParseLog> AllEventsBuilder<T, E> {
    /// Creates a new all events builder from a web3 provider and and address.
    pub fn new(web3: Web3<T>, address: Address, deployment_transaction: Option<H256>) -> Self {
        AllEventsBuilder {
            web3,
            filter: FilterBuilder::default().address(vec![address]),
            from_block: None,
            to_block: None,
            topics: TopicFilter::default(),
            poll_interval: None,
            deployment_transaction,
            block_page_size: None,
            _events: PhantomData,
        }
    }

    /// Sets the starting block from which to stream logs for.
    ///
    /// If left unset defaults to the latest block.
    #[allow(clippy::wrong_self_convention)]
    pub fn from_block(mut self, block: BlockNumber) -> Self {
        self.from_block = Some(block);
        self
    }

    /// Sets the last block from which to stream logs for.
    ///
    /// If left unset defaults to the streaming until the end of days.
    #[allow(clippy::wrong_self_convention)]
    pub fn to_block(mut self, block: BlockNumber) -> Self {
        self.to_block = Some(block);
        self
    }

    /// Limit the number of events that can be retrieved by this filter.
    ///
    /// Note that this parameter is non-standard.
    pub fn limit(mut self, value: usize) -> Self {
        self.filter = self.filter.limit(value);
        self
    }

    /// Adds a filter for the first indexed topic.
    ///
    /// For regular events, this corresponds to the event signature. For
    /// anonymous events, this is the first indexed property.
    pub fn topic0(mut self, topic: Topic<H256>) -> Self {
        self.topics.topic0 = topic;
        self
    }

    /// Adds a filter for the second indexed topic.
    pub fn topic1(mut self, topic: Topic<H256>) -> Self {
        self.topics.topic1 = topic;
        self
    }

    /// Adds a filter for the third indexed topic.
    pub fn topic2(mut self, topic: Topic<H256>) -> Self {
        self.topics.topic2 = topic;
        self
    }

    /// Adds a filter for the third indexed topic.
    pub fn topic3(mut self, topic: Topic<H256>) -> Self {
        self.topics.topic2 = topic;
        self
    }

    /// The polling interval. This is used as the interval between consecutive
    /// `eth_getFilterChanges` calls to get filter updates.
    pub fn poll_interval(mut self, value: Duration) -> Self {
        self.poll_interval = Some(value);
        self
    }

    /// The page size in blocks to use when doing a paginated query on past
    /// events.
    pub fn block_page_size(mut self, value: u64) -> Self {
        self.block_page_size = Some(value);
        self
    }

    /// Returns a web3 provider and filter needed for querying and streaming
    /// events.
    fn prepare(self) -> (Web3<T>, FilterBuilder) {
        let mut filter_builder = self.filter.topic_filter(self.topics);
        if let Some(from_block) = self.from_block {
            filter_builder = filter_builder.from_block(from_block);
        }
        if let Some(to_block) = self.to_block {
            filter_builder = filter_builder.to_block(to_block);
        }

        (self.web3, filter_builder)
    }

    /// Returns a future that resolves into a collection of events matching the
    /// event builder's parameters.
    pub fn query(self) -> QueryAllFuture<T, E> {
        QueryAllFuture::from_builder(self)
    }

    /// Returns a future that resolves into a collection of events matching the
    /// event builder's parameters. This method is similar to `query` with the
    /// notable difference that the logs are fetched in pages by querying
    /// smaller block ranges specified by `block_page_size` instead of using a
    /// single query.
    ///
    /// Note that if the block range is inconsistent (for example from block is
    /// after the to block, or querying until the earliest block), then the
    /// query will be forwarded to the node as is.
    pub async fn query_past_events_paginated(self) -> Result<Vec<Event<E>>, ExecutionError> {
        let mut start_block = match self.from_block {
            None | Some(BlockNumber::Earliest) => 0,
            Some(BlockNumber::Number(value)) => value.as_u64(),
            Some(BlockNumber::Latest) | Some(BlockNumber::Pending) => {
                return self.query().await;
            }
        };
        if let Some(deployment_tx) = self.deployment_transaction {
            start_block = cmp::max(
                start_block,
                block_number_from_transaction_hash(self.web3.clone(), deployment_tx).await?,
            );
        }

        let end_block = match self.to_block {
            None | Some(BlockNumber::Latest) | Some(BlockNumber::Pending) => {
                self.web3.eth().block_number().compat().await?.as_u64()
            }
            Some(BlockNumber::Number(value)) => value.as_u64(),
            Some(BlockNumber::Earliest) => {
                return self.query().await;
            }
        };

        // NOTE: If the range is invalid, forward the request to the node to the
        //   node to make sure we behave consistently for these edge cases.
        if start_block > end_block {
            return self.query().await;
        }

        let page_size = self.block_page_size.unwrap_or(DEFAULT_BLOCK_PAGE_SIZE);
        let (web3, filter) = self.prepare();

        let mut events = Vec::new();
        let mut current_block = start_block;
        while current_block <= end_block {
            let filter = {
                let mut filter = filter.clone().from_block(current_block.into());

                // NOTE: The last page is handled a bit differently by using the
                //   `to_block` that was originally specified to the builder and
                //   is set in the `filter` (from the call to `prepare`). This
                //   is done in case the to block was "latest" or "pending",
                //   where we want to make sure that the last call includes
                //   blocks that have been added since the start of the
                //   paginated query.
                if current_block + page_size <= end_block {
                    filter = filter.to_block((current_block + page_size - 1).into());
                }

                filter.build()
            };

            let log_page = web3.eth().logs(filter).compat().await?;
            events.reserve(log_page.len());
            for log in log_page {
                let event = Event::from_log(log, E::parse_log)?;
                events.push(event);
            }

            current_block += page_size;
        }

        Ok(events)
    }

    /// Creates an event stream from the current event builder.
    pub fn stream(self) -> AllEventsStream<T, E> {
        AllEventsStream::from_builder(self)
    }
}

/// Retrieves a block number for the specified transaction hash.
async fn block_number_from_transaction_hash<T: Transport>(
    web3: Web3<T>,
    tx_hash: H256,
) -> Result<u64, ExecutionError> {
    let tx_receipt = web3
        .eth()
        .transaction_receipt(tx_hash)
        .compat()
        .await?
        .ok_or(ExecutionError::MissingTransaction(tx_hash))?;
    Ok(tx_receipt
        .block_number
        .ok_or(ExecutionError::PendingTransaction(tx_hash))?
        .as_u64())
}

/// A future for querying all contract events based on a log filter.
#[must_use = "futures do nothing unless you await or poll them"]
#[pin_project]
pub struct QueryAllFuture<T: Transport, E: ParseLog> {
    #[pin]
    inner: CompatCallFuture<T, Vec<Log>>,
    _event: PhantomData<E>,
}

impl<T: Transport, E: ParseLog> QueryAllFuture<T, E> {
    /// Create a new query future from event builder parameters.
    pub fn from_builder(builder: AllEventsBuilder<T, E>) -> Self {
        let (web3, filter) = builder.prepare();
        let inner = web3.eth().logs(filter.build()).compat();

        QueryAllFuture {
            inner,
            _event: PhantomData,
        }
    }
}

impl<T: Transport, E: ParseLog> Future for QueryAllFuture<T, E> {
    type Output = Result<Vec<Event<E>>, ExecutionError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.project().inner.poll(cx).map(|logs| {
            logs?
                .into_iter()
                .map(|log| Event::from_log(log, E::parse_log))
                .collect::<Result<Vec<_>, ExecutionError>>()
        })
    }
}

/// An event stream for all contract events.
#[must_use = "streams do nothing unless you or poll them"]
#[pin_project]
pub struct AllEventsStream<T: Transport, E: ParseLog> {
    #[pin]
    inner: LogStream<T>,
    _events: PhantomData<E>,
}

impl<T: Transport, E: ParseLog> AllEventsStream<T, E> {
    /// Create a new log stream from a given web3 provider, filter and polling
    /// parameters.
    pub fn from_builder(builder: AllEventsBuilder<T, E>) -> Self {
        let poll_interval = builder.poll_interval.unwrap_or(DEFAULT_POLL_INTERVAL);
        let (web3, filter) = builder.prepare();
        let inner = LogStream::new(web3, filter.build(), poll_interval);

        AllEventsStream {
            inner,
            _events: PhantomData,
        }
    }
}

impl<T: Transport, E: ParseLog> Stream for AllEventsStream<T, E> {
    type Item = Result<Event<E>, ExecutionError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.project()
            .inner
            .poll_next(cx)
            .map(|next| next.map(|log| Event::from_log(log?, E::parse_log)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::prelude::*;
    use ethcontract_common::abi::{EventParam, ParamType};
    use futures::stream::StreamExt;
    use serde_json::Value;
    use web3::types::{Address, H2048, H256, U256, U64};

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
    fn event_query() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());
        let (event, log) = test_abi_event();

        // get logs filter
        transport.add_response(json!([log]));

        let address = Address::repeat_byte(0x01);
        let signature = event.signature();
        let events = EventBuilder::<_, (Address, Address, U256)>::new(web3, event, address)
            .to_block(99.into())
            .limit(1000)
            .topic1(Topic::OneOf(vec![
                Address::repeat_byte(0x70),
                Address::repeat_byte(0x80),
            ]))
            .query()
            .expect("failed to abi-encode filter")
            .immediate()
            .expect("failed to get logs");

        assert!(events[0].is_added());
        assert_eq!(events[0].inner_data().2, U256::from(42));
        transport.assert_request(
            "eth_getLogs",
            &[json!({
                "address": address,
                "toBlock": U256::from(99),
                "limit": 1000,
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
        transport.assert_no_more_requests();
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
        let event = EventBuilder::<_, (Address, Address, U256)>::new(web3, event, address)
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
            .expect("failed to get log from log stream");

        assert!(event.is_added());
        assert_eq!(event.inner_data().2, U256::from(42));
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

    #[test]
    fn all_events_query() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());
        let (event, log) = test_abi_event();

        // get logs
        transport.add_response(json!([log]));

        let address = Address::repeat_byte(0x01);
        let signature = event.signature();
        let raw_events = AllEventsBuilder::<_, RawLog>::new(web3, address, None)
            .to_block(99.into())
            .topic0(Topic::This(signature))
            .topic2(Topic::OneOf(vec![
                Address::repeat_byte(0x70).into(),
                Address::repeat_byte(0x80).into(),
            ]))
            .query()
            .immediate()
            .expect("failed to get logs");

        assert!(raw_events[0].is_added());
        assert_eq!(
            *raw_events[0].inner_data(),
            RawLog {
                topics: vec![
                    signature,
                    Address::repeat_byte(0xf0).into(),
                    Address::repeat_byte(0x70).into(),
                ],
                data: {
                    let mut buf = vec![0u8; 32];
                    buf[31] = 42;
                    buf
                },
            },
        );
        transport.assert_request(
            "eth_getLogs",
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
        transport.assert_no_more_requests();
    }

    #[test]
    fn all_events_query_paginated() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());
        let (event, log) = test_abi_event();

        let address = Address::repeat_byte(0x01);
        let deployment = H256::repeat_byte(0x42);
        let signature = event.signature();

        // get tx receipt for past blocks
        transport.add_response(json!({
            "transactionHash": deployment,
            "transactionIndex": "0x1",
            "blockNumber": U64::from(10),
            "blockHash": H256::zero(),
            "cumulativeGasUsed": "0x1337",
            "gasUsed": "0x1337",
            "logsBloom": H2048::zero(),
            "logs": [],
        }));
        // get latest block
        transport.add_response(json!(U64::from(20)));
        // get logs pages
        transport.add_response(json!([log]));
        transport.add_response(json!([]));
        transport.add_response(json!([log, log]));

        let raw_events = AllEventsBuilder::<_, RawLog>::new(web3, address, Some(deployment))
            .from_block(5.into())
            .to_block(BlockNumber::Pending)
            .topic0(Topic::This(signature))
            .block_page_size(5)
            .query_past_events_paginated()
            .immediate()
            .expect("failed to get logs");

        assert_eq!(raw_events.len(), 3);
        transport.assert_request("eth_getTransactionReceipt", &[json!(deployment)]);
        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request(
            "eth_getLogs",
            &[json!({
                "address": address,
                "fromBlock": U64::from(10),
                "toBlock": U64::from(14),
                "topics": [signature],
            })],
        );
        transport.assert_request(
            "eth_getLogs",
            &[json!({
                "address": address,
                "fromBlock": U64::from(15),
                "toBlock": U64::from(19),
                "topics": [signature],
            })],
        );
        transport.assert_request(
            "eth_getLogs",
            &[json!({
                "address": address,
                "fromBlock": U64::from(20),
                "toBlock": "pending",
                "topics": [signature],
            })],
        );
        transport.assert_no_more_requests();
    }

    #[test]
    fn all_events_stream_next_event() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());
        let (event, log) = test_abi_event();

        // filter created
        transport.add_response(json!("0xf0"));
        // get logs filter
        transport.add_response(json!([log]));

        let address = Address::repeat_byte(0x01);
        let signature = event.signature();
        let raw_event = AllEventsBuilder::<_, RawLog>::new(web3, address, None)
            .to_block(99.into())
            .topic0(Topic::This(signature))
            .topic2(Topic::OneOf(vec![
                Address::repeat_byte(0x70).into(),
                Address::repeat_byte(0x80).into(),
            ]))
            .stream()
            .next()
            .immediate()
            .expect("log stream did not produce any logs")
            .expect("failed to get log from log stream");

        assert!(raw_event.is_added());
        assert_eq!(
            *raw_event.inner_data(),
            RawLog {
                topics: vec![
                    signature,
                    Address::repeat_byte(0xf0).into(),
                    Address::repeat_byte(0x70).into(),
                ],
                data: {
                    let mut buf = vec![0u8; 32];
                    buf[31] = 42;
                    buf
                },
            },
        );
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
