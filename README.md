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

## Running the Example

In order to run the example you need:
- Rust >=1.39 for `async`/`await` support
- NodeJS in order to compile truffle contracts and start development node

```sh
$ cd examples/truffle
$ npm run build
$ npm run develop
```

Then in a sepate terminal window, you can run the example:

```sh
$ cargo run --example async
```

This example deploys a ERC20 token and interacts with the contract with various
accounts.

## Sample Contracts Documentation

We added some samples of generated contracts from our sample contract collection
gated behind a feature. This feature is **only intended to be used for document
generation**. In order to view the documentation for these contracts you need to
first build the contracts and then generate documentation for the crate with the
`samples` feature enabled:

```sh
$ (cd examples/truffle; npm run build)
$ cargo doc --features samples --open
```

This will open a browser at the documentation root. Look under the `samples`
module for the sample contracts to get a feel for how the generated types look.
