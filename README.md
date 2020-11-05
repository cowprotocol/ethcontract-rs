[![Build Status](https://travis-ci.org/gnosis/ethcontract-rs.svg?branch=main)](https://travis-ci.org/gnosis/ethcontract-rs)
[![Crates.io](https://img.shields.io/crates/v/ethcontract.svg)](https://crates.io/crates/ethcontract)
[![Docs.rs](https://docs.rs/ethcontract/badge.svg)](https://docs.rs/ethcontract)
[![Rustc Version](https://img.shields.io/badge/rustc-1.47+-lightgray.svg)](https://blog.rust-lang.org/2019/12/19/Rust-1.47.0.html)

# `ethcontract-rs`

Crate used for generating code for Ethereum smart contracts. It provides a
function procedural macro that generates safe bindings for contract interaction
based on the contract ABI.

## Getting Started

Add a dependency to the `ethcontract` crate in your `Cargo.toml`:

```toml
[dependencies]
ethcontract = "..."
```

Then generate a struct for interacting with the smart contract with a type-safe
API:

```rust
ethcontract::contract!("path/to/truffle/build/contract/Contract.json");
```

This will generate a new struct `ContractName` with contract generated methods
for interacting with contract functions in a type-safe way.

### Minimum Supported Rust Version

The minimum supported Rust version is 1.42.

## Generator API

As an alternative to the procedural macro, a generator API is provided for
generating contract bindings from `build.rs` scripts. More information can be
found in the `ethcontract-generate` [README](generate/README.md).

## Running the Examples

In order to run local examples you will additionally need:
- NodeJS LTS
- Yarn

For all examples, the smart contracts must first be built:

```sh
cd examples/truffle
yarn && yarn build
```

### Truffle Examples

Truffle examples rely on the local truffle development server. In a separate
terminal run:

```sh
cd examples/truffle
yarn start
```

#### ABI:

The `abi` example deploys a simple contract and performs various `eth_call`s
to illustrate how Solidity types are mapped to Rust types by `ethcontract`.

```sh
cargo run --example abi
```

#### Async/Await:

The `async` example deploys an ERC20 token and interacts with the contract
with various accounts.

```sh
cargo run --example async
```

#### Manual Deployments:

The `deployments` example illustrates how the `deployments` parameter can be
specified when generating a contract with the `ethcontract::contract!` macro.
This can be useful for specifying addresses in testing environments that are
deterministic but either not included, or inaccurate in the artifact's
`networks` property (when for example the contract is developed upstream, but
a separate testnet deployment wants to be used).

```sh
cargo run --example deployments
```

#### Events:

The `events` example illustrates how to listen to logs emitted by smart
contract events.

```sh
cargo run --example events
```

#### Generator API (with `build.rs` script):

The `generator` example (actually a separate crate to be able to have a build
script) demonstrates how the generator API can be used for creating type-safe
bindings to a smart contract with a `build.rs` build script.

```sh
cargo run --package examples-generate
```

#### Contract Linking:

The `linked` example deploys a library and a contract that links to it then
makes a method call.

```sh
cargo run --example linked
```

### Rinkeby Example

There is a provided example that runs with Rinkeby and Infura. Running this
example is a little more involved to run because it requires a private key with
funds on Rinkeby (for gas) as well as an Infura project ID in order to connect
to a node. Parameters are provided to the Rinkeby example by environment
variables:

```sh
export PK="private key"
export INFURA_PROJECT_ID="Infura project ID"
cargo run --example rinkeby
```

### Mainnet Examples

#### Sources:

This example generates contract bindings from online sources:
- A verified contract on Etherscan
- An npmjs contract

It also queries some contract state with Infura. Running this example requires
an Infura project ID in order to connect to a node. Parameters are provided to
the example by environment variables:

```sh
export INFURA_PROJECT_ID="Infura project ID"
cargo run --example sources
```

#### Past Events:

This example retrieves the entire event history of token OWL contract and prints
the total number of events since deployment.

Note the special handling of the `tokenOWLProxy` contract and how it is cast into
a `tokenOWL` instance using Contract's `with_transaction` feature.

```sh
export INFURA_PROJECT_ID="Infura project ID"
cargo run --example past_events
```

## Sample Contracts Documentation

We added some samples of generated contracts from our sample contract collection
gated behind a feature. This feature is **only intended to be used for document
generation**. In order to view the documentation for these contracts you need to
first build the contracts and then generate documentation for the crate with the
`samples` feature enabled:

```sh
(cd examples/truffle; yarn && yarn build)
cargo doc --features samples --open
```

This will open a browser at the documentation root. Look under the `samples`
module for the sample contracts to get a feel for how the generated types look.
