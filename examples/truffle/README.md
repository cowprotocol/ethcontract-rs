# Truffle

This subdirectory contains a truffle project with sample contracts used by the
`ethcontract-rs` crate for its examples.

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

This contract can be built with truffle. There is an NPM script for doing this:

```sh
$ npm run build
```

## Development Server

We use `truffle develop` for the development server (which uses ganache under
the hood). The configuration for the development server is located in the
`truffle-config.js` file.
