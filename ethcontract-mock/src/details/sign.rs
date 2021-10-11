//! Helpers to work with signed transactions.

use crate::details::transaction::Transaction;
use ethcontract::common::abi::ethereum_types::BigEndianHash;
use ethcontract::web3::signing;
use ethcontract::web3::types::{Address, H256, U256};

/// Parses and verifies raw transaction, including chain ID.
///
/// Panics if transaction is malformed or if verification fails.
pub fn verify(raw_tx: &[u8], node_chain_id: u64) -> Transaction {
    let rlp = rlp::Rlp::new(raw_tx);

    fn err() -> ! {
        panic!("invalid transaction data");
    }
    fn res<T, E>(r: Result<T, E>) -> T {
        r.unwrap_or_else(|_| err())
    }

    if !matches!(rlp.prototype(), Ok(rlp::Prototype::List(9))) {
        err();
    }

    if res(rlp.at(3)).size() == 0 {
        // TODO:
        //
        // We could support deployments via RPC calls by introducing
        // something like `expect_deployment` method to `Mock` struct.
        panic!("mock client does not support deploying contracts via transaction, use `Mock::deploy` instead");
    }

    let nonce: U256 = res(rlp.val_at(0));
    let gas_price: U256 = res(rlp.val_at(1));
    let gas: U256 = res(rlp.val_at(2));
    let to: Address = res(rlp.val_at(3));
    let value: U256 = res(rlp.val_at(4));
    let data: Vec<u8> = res(rlp.val_at(5));
    let v: u64 = res(rlp.val_at(6));
    let r = H256::from_uint(&res(rlp.val_at(7)));
    let s = H256::from_uint(&res(rlp.val_at(8)));

    let (chain_id, standard_v) = match v {
        v if v >= 35 => ((v - 35) / 2, (v - 25) % 2),
        27 | 28 => panic!("transactions must use eip-155 signatures"),
        _ => panic!("invalid transaction signature, v value is out of range"),
    };

    if chain_id != node_chain_id {
        panic!("invalid transaction signature, chain id mismatch");
    }

    let msg_hash = {
        let mut rlp = rlp::RlpStream::new();

        rlp.begin_list(9);
        rlp.append(&nonce);
        rlp.append(&gas_price);
        rlp.append(&gas);
        rlp.append(&to);
        rlp.append(&value);
        rlp.append(&data);
        rlp.append(&chain_id);
        rlp.append(&0u8);
        rlp.append(&0u8);

        signing::keccak256(rlp.as_raw())
    };

    let signature = {
        let mut signature = [0u8; 64];
        signature[..32].copy_from_slice(r.as_bytes());
        signature[32..].copy_from_slice(s.as_bytes());
        signature
    };

    let from = signing::recover(&msg_hash, &signature, standard_v as _)
        .unwrap_or_else(|_| panic!("invalid transaction signature, verification failed"));

    Transaction {
        from,
        to,
        nonce,
        gas,
        gas_price,
        value,
        data,
        hash: signing::keccak256(raw_tx).into(),
        transaction_type: 0,
        max_fee_per_gas: Default::default(),
        max_priority_fee_per_gas: Default::default(),
    }
}
