# `ethcontract-rs`

Crate used for generating code for Ethereum smart contracts. It provides a
function procedural macro that generates safe bindings for contract interaction
based on the contract ABI.

## Getting Started

Add a dependency to the `ethcontract` crate in your `Cargo.toml`:

```toml
# ...
[dependencies]
ethcontract = "..."
# ...
```

Then generate a struct for interacting with the smart contract with a type-safe
API:

```rust
ethcontract::contract!("path/to/truffle/build/contract/Contract.json");
```

This will generate a new struct `ContractName` with contract generated methods
for interacting with contract functions in a type-safe way.

## TODO

- [ ] PR for `web3` to move transaction signing code upstream (`account` ns)
- [ ] PR for `ethabi` so info about fallback function is provided (eg. payable)
- [ ] PR for `ethabi` add payable information to contract ABI
- [ ] PR for `web3` to implement `Tokenizable` on more types
- [ ] Add method for invoking fallback function
- [ ] Setup contract events as `futures::future::Stream`
- [ ] Add options for preserving ident cases (we idiomatically convert them ATM)
- [ ] Add options for creating generic structs (instead of DynTransport)
- [ ] Strategy for name collision
