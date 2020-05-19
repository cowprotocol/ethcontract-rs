//! Module contains code for parsing and manipulating event data.

use crate::errors::ExecutionError;
use ethcontract_common::abi::{Event as AbiEvent, RawLog as AbiRawLog};
use web3::contract::tokens::Detokenize;
use web3::types::{Log, H256};

/// A contract event
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Event<T> {
    /// The decoded log data.
    pub data: EventData<T>,
    /// The additional metadata for the event. Note that this is not always
    /// available if these logs are pending. This can happen if the `to_block`
    /// option was set to `BlockNumber::Pending`.
    pub meta: Option<EventMetadata>,
}

/// A type representing a contract event that was either added or removed. Note
/// that this type intentionally an enum so that the handling of removed events
/// is made more explicit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventData<T> {
    /// A new event was received.
    Added(T),
    /// A previously mined event was removed as a result of a re-org.
    Removed(T),
}

impl<T> Event<T> {
    /// Creates an event from a log given a mapping function.
    pub(crate) fn from_log<E, F>(log: Log, f: F) -> Result<Self, E>
    where
        F: FnOnce(RawLog) -> Result<T, E>,
    {
        let meta = EventMetadata::from_log(&log);
        let data = {
            let removed = log.removed == Some(true);
            let raw = RawLog::from(log);
            let inner_data = f(raw)?;

            if removed {
                EventData::Removed(inner_data)
            } else {
                EventData::Added(inner_data)
            }
        };

        Ok(Event { data, meta })
    }

    /// Get a reference the underlying event data regardless of whether the
    /// event was added or removed.
    pub fn inner_data(&self) -> &T {
        match &self.data {
            EventData::Added(value) => value,
            EventData::Removed(value) => value,
        }
    }

    /// Gets a bool representing if the event was added.
    pub fn is_added(&self) -> bool {
        matches!(&self.data, EventData::Added(_))
    }

    /// Gets a bool representing if the event was removed.
    pub fn is_removed(&self) -> bool {
        matches!(&self.data, EventData::Removed(_))
    }

    /// Get the underlying event data if the event was added, `None` otherwise.
    pub fn added(self) -> Option<T> {
        match self.data {
            EventData::Added(value) => Some(value),
            EventData::Removed(_) => None,
        }
    }

    /// Get the underlying event data if the event was removed, `None`
    /// otherwise.
    pub fn removed(self) -> Option<T> {
        match self.data {
            EventData::Removed(value) => Some(value),
            EventData::Added(_) => None,
        }
    }

    /// Maps the inner data of an event into some other data.
    pub fn map<U, F>(self, f: F) -> Event<U>
    where
        F: FnOnce(T) -> U,
    {
        Event {
            data: match self.data {
                EventData::Added(inner) => EventData::Added(f(inner)),
                EventData::Removed(inner) => EventData::Removed(f(inner)),
            },
            meta: self.meta,
        }
    }
}

/// Additional metadata from the log for the event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventMetadata {
    /// The hash of the block where the log was produced.
    pub block_hash: H256,
    /// The number of the block where the log was produced.
    pub block_number: u64,
    /// The hash of the transaction this log belongs to.
    pub transaction_hash: H256,
    /// The block index of the transaction this log belongs to.
    pub transaction_index: usize,
    /// The index of the log in the block.
    pub log_index: usize,
    /// The log index in the transaction this log belongs to. This property is
    /// non-standard.
    pub transaction_log_index: Option<usize>,
    /// The log type. Note that this property is non-standard but is supported
    /// by Parity nodes.
    pub log_type: Option<String>,
}

impl EventMetadata {
    fn from_log(log: &Log) -> Option<Self> {
        Some(EventMetadata {
            block_hash: log.block_hash?,
            block_number: log.block_number?.as_u64(),
            transaction_hash: log.transaction_hash?,
            transaction_index: log.transaction_index?.as_usize(),
            log_index: log.log_index?.as_usize(),
            transaction_log_index: log.transaction_log_index.map(|index| index.as_usize()),
            log_type: log.log_type.clone(),
        })
    }
}

/// Raw log topics and data for a contract event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawLog {
    /// The raw 32-byte topics.
    pub topics: Vec<H256>,
    /// The raw non-indexed data attached to an event.
    pub data: Vec<u8>,
}

impl RawLog {
    /// Decode raw log data into a tokenizable for a matching event ABI entry.
    pub fn decode<D>(self, event: &AbiEvent) -> Result<D, ExecutionError>
    where
        D: Detokenize,
    {
        let event_log = event.parse_log(AbiRawLog {
            topics: self.topics,
            data: self.data,
        })?;

        let tokens = event_log
            .params
            .into_iter()
            .map(|param| param.value)
            .collect::<Vec<_>>();
        let data = D::from_tokens(tokens)?;

        Ok(data)
    }
}

impl From<Log> for RawLog {
    fn from(log: Log) -> Self {
        RawLog {
            topics: log.topics,
            data: log.data.0,
        }
    }
}

/// Trait for parsing a transaction log into an some event data when the
/// expected event type is not known.
pub trait ParseLog: Sized {
    /// Create a new instance by parsing raw log data.
    fn parse_log(log: RawLog) -> Result<Self, ExecutionError>;
}

impl ParseLog for RawLog {
    fn parse_log(log: RawLog) -> Result<Self, ExecutionError> {
        Ok(log)
    }
}
