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

## Running the Examples

In order to run local examples you need:
- Rust >=1.39 for `async`/`await` support
- NodeJS in order to compile truffle contracts and, depending on the example,
  start development node

For all examples, the smart contracts must first be built:

```sh
$ cd examples/truffle
$ npm run build
```

### Async Example

This example deploys a ERC20 token and interacts with the contract with various
accounts. First start the local development server:

```sh
$ npm run develop
```

Then in a sepate terminal window, you can run the example:

```sh
$ cargo run --example async
```

### Rinkeby Example

There is a provided example that runs with Rinkeby and Infura. Running this
example is a little more involved to run because it requires a private key with
funds on Rinkeby (for gas) as well as a Infura project ID in order to connect to
a node. Parameters are provided to the Rinkeby example by environment variables:

```sh
$ export PK="private key"
$ export INFURA_PROJECT_ID="Infura project ID"
$ cargo run --example rinkeby
```

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
