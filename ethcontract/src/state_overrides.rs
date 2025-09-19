//! Contains data structures needed to specify
//! state overrides for `eth_call`s.

use primitive_types::{H160, H256, U256};
use serde::Serialize;
use std::collections::HashMap;
use web3::types::{Bytes, U64};

/// State overrides.
pub type StateOverrides = HashMap<H160, StateOverride>;

/// State override object.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateOverride {
    /// Fake balance to set for the account before executing the call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance: Option<U256>,

    /// Fake nonce to set for the account before executing the call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<U64>,

    /// Fake EVM bytecode to inject into the account before executing the call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<Bytes>,

    /// Fake key-value mapping to override **all** slots in the account storage
    /// before executing the call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<HashMap<H256, H256>>,

    /// Fake key-value mapping to override **individual** slots in the account
    /// storage before executing the call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_diff: Option<HashMap<H256, H256>>,
}
