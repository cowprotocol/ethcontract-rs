//! Module implements type-safe event streams from an ABI event definition with
//! detokenization of the data included in the log.

#![allow(dead_code)]

use ethcontract_common::abi::{Event, Topic, TopicFilter};
use web3::api::Web3;
use web3::types::Address;
use web3::Transport;

/// A builder for creating a filtered stream of contract events that are
pub struct EventBuilder<T: Transport> {
    web3: Web3<T>,
    event: Event,
    address: Address,
    pub topics: TopicFilter,
}

impl<T: Transport> EventBuilder<T> {
    pub fn new(web3: Web3<T>, event: Event, address: Address) -> Self {
        let topic0 = Topic::This(event.signature());

        EventBuilder {
            web3,
            event,
            address,
            topics: TopicFilter {
                topic0,
                ..Default::default()
            },
        }
    }
}
