//! Tests for mock crate.
//!
//! # TODO
//!
//! Some tests for API are missing:
//!
//! - malformed input in
//!   - eth_call
//!   - eth_sendTransaction
//!   - eth_sendRawTransaction
//!   - eth_estimateGas
//!
//! - deployment works
//! - different contracts have different addresses
//! - returned instance has correct address
//!
//! - call to method with no expectations panics
//! - tx to method with no expectations panics
//! - call to method with an expectation succeeds
//! - tx to method with an expectation succeeds
//!
//! - call expectations only match calls
//! - tx expectations only match txs
//! - regular expectations match both calls and txs
//!
//! - predicate filters expectation so test panics
//! - predicate filters multiple expectations so test panics
//! - expectations are evaluated in FIFO order
//! - predicate_fn gets called
//! - predicate_fn_ctx gets called
//!
//! - times can be set for expectation
//! - if expectation called not enough times, test panics
//! - if expectation called enough times, test passes
//! - if expectation called enough times, it is satisfied and test panics
//! - if expectation called enough times, it is satisfied and next expectation is used
//! - expectation is not satisfied if calls are not matched by a predicate
//!
//! - expectations can be added to sequences
//! - expectation can only be in one sequence
//! - adding expectation to sequence requires exact time greater than zero
//! - updating times after expectation was set requires exact time greater than zero
//! - when method called in-order, test passes
//! - when method called in-order multiple times, test passes
//! - when method called out-of-order, test panics
//! - when method called out-of-order first time with times(2), test panics
//! - when method called out-of-order last time with times(2), test panics
//!
//! - default value for solidity type is returned
//! - rust's Default trait is not honored
//! - you can set return value
//! - returns_fn gets called
//! - returns_fn_ctx gets called
//!
//! - expectations become immutable after use in calls and txs
//! - expectations become immutable after use in calls and txs even if they are not matched by a predicate
//! - new expectations are not immutable
//!
//! - checkpoint verifies expectations
//! - checkpoint clears expectations
//! - expectations become invalid
//!
//! - confirmations plays nicely with tx.confirmations

use crate::utils::*;
use crate::{Contract, Mock};
use ethcontract::dyns::DynWeb3;
use ethcontract::prelude::*;
use predicates::prelude::*;

mod batch;
mod eth_block_number;
mod eth_chain_id;
mod eth_estimate_gas;
mod eth_gas_price;
mod eth_get_transaction_receipt;
mod eth_send_transaction;
mod eth_transaction_count;

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

ethcontract::contract!("examples/truffle/build/contracts/ERC20.json");

fn setup() -> (Mock, DynWeb3, Contract, ERC20) {
    let mock = Mock::new(1234);
    let web3 = mock.web3();
    let contract = mock.deploy(ERC20::raw_contract().abi.clone());
    let mut instance = ERC20::at(&web3, contract.address);
    instance.defaults_mut().from = Some(account_for("Alice"));

    (mock, web3, contract, instance)
}

#[tokio::test]
async fn general_test() {
    let mock = crate::Mock::new(1234);
    let contract = mock.deploy(ERC20::raw_contract().abi.clone());

    let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    let mut sequence = mockall::Sequence::new();

    contract
        .expect(ERC20::signatures().balance_of())
        .once()
        .predicate((predicate::eq(address_for("Bob")),))
        .returns(U256::from(0))
        .in_sequence(&mut sequence);

    contract
        .expect(ERC20::signatures().transfer())
        .once()
        .predicate_fn_ctx(|ctx, _| !ctx.is_view_call)
        .returns_fn_ctx({
            let called = called.clone();
            move |ctx, (recipient, amount)| {
                assert_eq!(ctx.from, address_for("Alice"));
                assert_eq!(ctx.nonce.as_u64(), 0);
                assert_eq!(ctx.gas.as_u64(), 1);
                assert_eq!(ctx.gas_price.as_u64(), 1);
                assert_eq!(recipient, address_for("Bob"));
                assert_eq!(amount.as_u64(), 100);

                called.store(true, std::sync::atomic::Ordering::Relaxed);

                Ok(true)
            }
        })
        .confirmations(3)
        .in_sequence(&mut sequence);

    contract
        .expect(ERC20::signatures().balance_of())
        .once()
        .predicate((predicate::eq(address_for("Bob")),))
        .returns(U256::from(100))
        .in_sequence(&mut sequence);

    contract
        .expect(ERC20::signatures().balance_of())
        .predicate((predicate::eq(address_for("Alice")),))
        .returns(U256::from(100000));

    let actual_contract = ERC20::at(&mock.web3(), contract.address);

    let balance = actual_contract
        .balance_of(address_for("Bob"))
        .call()
        .await
        .unwrap();
    assert_eq!(balance.as_u64(), 0);

    assert!(!called.load(std::sync::atomic::Ordering::Relaxed));
    actual_contract
        .transfer(address_for("Bob"), U256::from(100))
        .from(account_for("Alice"))
        .confirmations(3)
        .send()
        .await
        .unwrap();
    assert!(called.load(std::sync::atomic::Ordering::Relaxed));

    let balance = actual_contract
        .balance_of(address_for("Bob"))
        .call()
        .await
        .unwrap();
    assert_eq!(balance.as_u64(), 100);

    let balance = actual_contract
        .balance_of(address_for("Alice"))
        .call()
        .await
        .unwrap();
    assert_eq!(balance.as_u64(), 100000);
}
