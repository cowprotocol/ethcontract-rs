# Truffle

This subdirectory contains a truffle project with sample contracts used by the
`ethcontract-rs` crate for its examples.

- `AbiTypes.sol` a simple contract that just returns pseudo-random data for
  various primitive Solidity types.
- `DeployedContract.sol` a simple contract that is deployed on the Rinkeby
  testnet and used for the rinkeby example.
- `DocumentedContract.sol` a sample with contract level documentation. We use
  this to verify the `ethcontract-derive` is properly injecting the docstring
  for the generated struct.
- `SampleLibrary.sol` and `LinkedContract.sol` a sample library and contract
  which uses the aforementioned library. We use this to test that linking and
  deployment with linking works.
- `RustCoin.sol` a sample ERC20 coin that we interact with in our async example.
  The example shows how to call contract functions and sign them with various
  strategies (offline, on the node, etc.).

## Building

This contract can be built with truffle. There is an npm script for doing this:

```sh
yarn run build
```

## Development Server

We use `truffle develop` for the development server (which uses ganache under
the hood). This is needed to run most examples.

## Rinkeby Deployment

The `DeployedContract` used in the rinkeby example must be deployed prior for
the example to work as expected. For this to work a few secrets are needed.
These are provided to truffle with environment variables:

```sh
export PK="private key"
export INFURA_PROJECT_ID="Infura project ID"
export ETHERSCAN_API_KEY="Etherscan API key"
```

In order to deploy, the following npm script should be used:
```sh
yarn run deploy
```

This will:
1. Build and deploy the contract using `$PK`'s account for paying gas fees and
  the `$INFURA_PROJECT_ID` to connect to a node
2. Verify the contract on Etherscan.io
