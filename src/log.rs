//! This module implements event builders and streams for retrieving events
//! emitted by a contract.

use crate::errors::ExecutionError;
use ethcontract_common::abi::{Topic, TopicFilter};
use futures::compat::{Compat01As03, Future01CompatExt, Stream01CompatExt};
use futures::future;
use futures::ready;
use futures::stream::{self, Stream, TryStreamExt};
use pin_project::{pin_project, project};
use std::future::Future;
use std::num::NonZeroU64;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use web3::api::{CreateFilter, FilterStream, Web3};
use web3::types::{Address, BlockNumber, Filter, FilterBuilder, Log, H256};
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

/// The default poll interval to use for polling logs from the block chain.
#[cfg(not(test))]
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5);
#[cfg(test)]
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(0);

/// The default block page size used for querying past events.
pub const DEFAULT_BLOCK_PAGE_SIZE: u64 = 10_000;

/// The default number for confirmations before a log is no longer considered
/// when dealing with re-orgs.
pub const DEFAULT_CONFIRMATIONS: usize = 5;

/// A log filter builder for configuring either a query for past logs or a
/// stream that constantly queries new logs and deals with re-orgs.
#[derive(Debug)]
#[must_use = "log filter builders do nothing unless you query or stream them"]
pub struct LogFilterBuilder<T: Transport> {
    /// The underlying web3 provider used for retrieving logs.
    web3: Web3<T>,
    /// The block to start streaming logs from.
    ///
    /// See [`web3::types::BlockNumber`] for more details on possible values.
    pub from_block: Option<BlockNumber>,
    /// The block to stop streaming logs from.
    ///
    /// See [`web3::types::BlockNumber`] for more details on possible values.
    pub to_block: Option<BlockNumber>,
    /// The contract addresses to filter logs for.
    pub address: Vec<Address>,
    /// Topic filters used for filtering logs based on indexed topics.
    pub topics: TopicFilter,

    /// The page size in blocks to use when doing a paginated query on past
    /// logs. This provides no guarantee in how many logs will be returned per
    /// page, but used to limit the block range for the query.
    pub block_page_size: Option<NonZeroU64>,
    /// The number of blocks to confirm the logs with. This is the number of
    /// blocks mined on top of the block where the log was emitted for it to be
    /// considered confirmed.
    ///
    /// Live log streaming keeps track of events in the last `confirmations`
    /// blocks so that reorgs can be detected and a `Removed` logs can be
    /// emitted by the log stream. Omit this property for a sensible default.
    pub confirmations: Option<usize>,
    /// The polling interval for querying the node for more logs.
    pub poll_interval: Option<Duration>,
}

impl<T: Transport> LogFilterBuilder<T> {
    /// Creates a new log filter builder from the specified web3 provider.
    pub fn new(web3: Web3<T>) -> Self {
        LogFilterBuilder {
            web3,
            from_block: None,
            to_block: None,
            address: Vec::new(),
            topics: TopicFilter::default(),
            block_page_size: None,
            confirmations: None,
            poll_interval: None,
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

    /// Adds an address filter to only retrieve logs that were emitted by a
    /// contract matching the povided addresses.
    pub fn address(mut self, address: Vec<Address>) -> Self {
        self.address = address;
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
        self.topics.topic3 = topic;
        self
    }

    /// The page size in blocks to use when doing a paginated query on past
    /// events.
    ///
    /// # Panics
    ///
    /// Panics if a block page size of 0 is specified.
    pub fn block_page_size(mut self, value: u64) -> Self {
        self.block_page_size = Some(NonZeroU64::new(value).expect("block page size cannot be 0"));
        self
    }

    /// The number of blocks mined after a log has been emitted until it is
    /// considered confirmed and can no longer be reorg-ed.
    pub fn confirmations(mut self, value: usize) -> Self {
        self.confirmations = Some(value);
        self
    }

    /// The polling interval. This is used as the interval between consecutive
    /// `eth_getLogs` calls to get log updates.
    pub fn poll_interval(mut self, value: Duration) -> Self {
        self.poll_interval = Some(value);
        self
    }

    /// Returns a web3 filter builder needed for querying and streaming logs.
    pub fn into_filter(self) -> FilterBuilder {
        let mut filter = FilterBuilder::default();
        if !self.address.is_empty() {
            filter = filter.address(self.address);
        }
        if self.topics != TopicFilter::default() {
            filter = filter.topic_filter(self.topics)
        }
        if let Some(from_block) = self.from_block {
            filter = filter.from_block(from_block);
        }
        if let Some(to_block) = self.to_block {
            filter = filter.to_block(to_block);
        }

        filter
    }

    /// Performs a `eth_getLogs` query to past logs. For large block ranges,
    /// such as retrieving all contract logs since genesis, it is recommended to
    /// use the `past_logs_pages` method instead.
    pub async fn past_logs(self) -> Result<Vec<Log>, ExecutionError> {
        let web3 = self.web3.clone();
        let filter = self.into_filter();
        let logs = web3.eth().logs(filter.build()).compat().await?;

        Ok(logs)
    }

    /// Returns a stream that resolves into a page of logs matching the filter
    /// builder's parameters.
    pub fn past_logs_pages(self) -> impl Stream<Item = Result<Vec<Log>, ExecutionError>> {
        stream::try_unfold(PastLogsStream::Init(self), PastLogsStream::next)
            .try_filter(|logs| future::ready(!logs.is_empty()))
    }
}

/// Internal unfold context for creating a `past_logs` `Stream`.
enum PastLogsStream<T: Transport> {
    Init(LogFilterBuilder<T>),
    Done,
    Paging(PastLogsPager<T>),
    Querying(Web3<T>, Filter),
}

impl<T: Transport> PastLogsStream<T> {
    async fn next(mut self) -> Result<Option<(Vec<Log>, Self)>, ExecutionError> {
        loop {
            let (logs, next) = match self {
                PastLogsStream::Init(builder) => {
                    self = PastLogsStream::init(builder).await?;
                    continue;
                }
                PastLogsStream::Done => return Ok(None),
                PastLogsStream::Paging(mut pager) => {
                    let logs = match pager.next_page().await? {
                        Some(logs) => logs,
                        None => return Ok(None),
                    };
                    (logs, PastLogsStream::Paging(pager))
                }
                PastLogsStream::Querying(web3, filter) => {
                    let logs = web3.eth().logs(filter.clone()).compat().await?;
                    (logs, PastLogsStream::Done)
                }
            };
            return Ok(Some((logs, next)));
        }
    }

    async fn init(builder: LogFilterBuilder<T>) -> Result<Self, ExecutionError> {
        let from_block = builder.from_block.unwrap_or(BlockNumber::Latest);
        let to_block = builder.to_block.unwrap_or(BlockNumber::Latest);

        let web3 = builder.web3.clone();
        let block_page_size = builder
            .block_page_size
            .map(|size| size.get())
            .unwrap_or(DEFAULT_BLOCK_PAGE_SIZE);
        let filter = builder.into_filter();

        let start_block = match from_block {
            BlockNumber::Earliest => Some(0),
            BlockNumber::Number(value) => Some(value.as_u64()),
            BlockNumber::Latest | BlockNumber::Pending => None,
        };
        let end_block = match to_block {
            BlockNumber::Earliest => None,
            BlockNumber::Number(value) => Some(value.as_u64()),
            BlockNumber::Latest | BlockNumber::Pending => {
                let latest_block = web3.eth().block_number().compat().await?;
                Some(latest_block.as_u64())
            }
        };

        let next = match (start_block, end_block) {
            (Some(page_block), Some(end_block)) => PastLogsStream::Paging(PastLogsPager {
                web3,
                to_block,
                block_page_size,
                filter,
                page_block,
                end_block,
            }),
            _ => PastLogsStream::Querying(web3, filter.build()),
        };

        Ok(next)
    }
}

/// Internal state for paging though past logs.
struct PastLogsPager<T: Transport> {
    web3: Web3<T>,

    /// The `to_block` specified by the log filter.
    to_block: BlockNumber,
    /// The block page size being used for queries.
    block_page_size: u64,
    /// The web3 filter used for retrieving the logs.
    filter: FilterBuilder,

    /// The block number for the next page.
    ///
    /// This value can be `None` if it `from_block` was specified as `Latest` or
    /// `Pending`.
    page_block: u64,
    /// The last block used for pagination. This is slightly different than
    /// `to_block` as this must be a concrete block number (and can't be block
    /// aliases such as `Earliest` or `Latest`).
    ///
    /// This has a value of `None` if it has not yet been retrieved (in case
    /// `to_block` is `Latest` or `Pending`).
    end_block: u64,
}

impl<T: Transport> PastLogsPager<T> {
    async fn next_page(&mut self) -> Result<Option<Vec<Log>>, ExecutionError> {
        debug_assert!(
            self.block_page_size != 0,
            "pager should never be constructed with 0 block page size",
        );

        while self.page_block <= self.end_block {
            // NOTE: Log block ranges are inclusive.
            let page_end = self.page_block + self.block_page_size - 1;
            let page_to_block = if page_end < self.end_block {
                BlockNumber::Number(page_end.into())
            } else {
                // NOTE: The last page is handled a bit differently by using the
                //   `to_block` that was originally specified to the builder.
                //   This is done because the `end_block` is determined when the
                //   pager is created, and it is possible that the `to_block`
                //   was specified as "latest" or "pending" which may have
                //   changed since the paging started.
                self.to_block
            };

            let page = self
                .web3
                .eth()
                .logs(
                    self.filter
                        .clone()
                        .from_block(self.page_block.into())
                        .to_block(page_to_block)
                        .build(),
                )
                .compat()
                .await?;

            self.page_block = page_end + 1;
            if page.is_empty() {
                continue;
            }

            return Ok(Some(page));
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::prelude::*;
    use futures::stream::StreamExt;
    use serde_json::Value;
    use web3::types::U64;

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

    #[test]
    fn past_log_stream_logs() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let address = Address::repeat_byte(0x42);
        let topic = H256::repeat_byte(42);
        let log = generate_log("awesome");

        // get latest block
        transport.add_response(json!(U64::from(20)));
        // get logs pages
        transport.add_response(json!([log]));
        transport.add_response(json!([]));
        transport.add_response(json!([log, log]));

        let mut raw_events = LogFilterBuilder::new(web3)
            .from_block(10.into())
            .to_block(BlockNumber::Pending)
            .address(vec![address])
            .topic0(Topic::This(topic))
            .block_page_size(5)
            .past_logs_pages()
            .boxed();

        let next = raw_events.next().immediate();
        assert!(
            matches!(&next, Some(Ok(logs)) if logs.len() == 1),
            "expected page length of 1 but got {:?}",
            next,
        );

        let next = raw_events.next().immediate();
        assert!(
            matches!(&next, Some(Ok(logs)) if logs.len() == 2),
            "expected page length of 2 but got {:?}",
            next,
        );

        let next = raw_events.next().immediate();
        assert!(
            next.is_none(),
            "expected stream to be complete but got {:?}",
            next,
        );

        transport.assert_request("eth_blockNumber", &[]);
        transport.assert_request(
            "eth_getLogs",
            &[json!({
                "address": address,
                "fromBlock": U64::from(10),
                "toBlock": U64::from(14),
                "topics": [topic],
            })],
        );
        transport.assert_request(
            "eth_getLogs",
            &[json!({
                "address": address,
                "fromBlock": U64::from(15),
                "toBlock": U64::from(19),
                "topics": [topic],
            })],
        );
        transport.assert_request(
            "eth_getLogs",
            &[json!({
                "address": address,
                "fromBlock": U64::from(20),
                "toBlock": "pending",
                "topics": [topic],
            })],
        );
        transport.assert_no_more_requests();
    }
}
