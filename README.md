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
