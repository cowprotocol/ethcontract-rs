// Common types used in tests and doctests.
//
// This file is `include!`d by doctests, it is not a part of the crate.

use ethcontract::dyns::DynInstance;
use ethcontract::prelude::*;
use ethcontract_mock::{CallContext, Contract, Expectation, Mock, Signature};
use predicates::prelude::*;

fn simple_abi() -> ethcontract::common::Abi {
    static ABI: &str = r#"
        {
          "abi": [
            {
              "inputs": [
                {
                  "internalType": "uint256",
                  "name": "a",
                  "type": "uint256"
                },
                {
                  "internalType": "uint256",
                  "name": "b",
                  "type": "uint256"
                }
              ],
              "name": "Foo",
              "outputs": [
                {
                  "internalType": "uint256",
                  "name": "a",
                  "type": "uint256"
                }
              ],
              "stateMutability": "view",
              "type": "function"
            }
          ]
        }
    "#;

    ethcontract::common::artifact::truffle::TruffleLoader::new()
        .load_contract_from_str(ABI)
        .unwrap()
        .abi
}

fn voting_abi() -> ethcontract::common::Abi {
    static ABI: &str = r#"
        {
              "abi": [
            {
              "inputs": [
                {
                  "internalType": "uint256",
                  "name": "proposal",
                  "type": "uint256"
                }
              ],
              "name": "vote",
              "outputs": [],
              "stateMutability": "nonpayable",
              "type": "function"
            },
            {
              "inputs": [],
              "name": "winningProposal",
              "outputs": [
                {
                  "internalType": "uint256",
                  "name": "winningProposal_",
                  "type": "uint256"
                }
              ],
              "stateMutability": "view",
              "type": "function"
            }
          ]
        }
    "#;

    ethcontract::common::artifact::truffle::TruffleLoader::new()
        .load_contract_from_str(ABI)
        .unwrap()
        .abi
}

fn contract() -> Contract {
    Mock::new(10).deploy(simple_abi())
}

fn signature() -> Signature<(u64, u64), u64> {
    Signature::new([54, 175, 98, 158])
}

fn address_for(who: &str) -> Address {
    account_for(who).address()
}

fn account_for(who: &str) -> Account {
    use ethcontract::web3::signing::keccak256;
    Account::Offline(
        PrivateKey::from_raw(keccak256(who.as_bytes())).unwrap(),
        None,
    )
}
