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

## Yes, They Are Checked-in

So, unfortunately we have to check in the compiled contract artifacts so that
they can be used by docs and doc-tests on `Docs.rs` which cannot build truffle
projects.

This unfortunately leads to some complications as truffle output depends on a
few things that are un-related to the actual contract being compiled:
- There is a `updatedAt` for the truffle artifact that has to be stripped since
  it will change every time the contract is rebuilt.
- Absolute paths are included in various places in the artifact including the
  `metadata` property.
- Contract bytecode contains a hash of the metatada property that is dependant
  on the absolute path, so depending on where the contract is compiled, the
  final bytecode will be different!

Right now, the committed truffle artifact needs to be post-processed so that we
can check on the CI that the artifacts are up to date and prevent different
developers do not commit changes to the artifacts that are not actual changes.

This is a suboptimal solution and we are open to suggestion improvements!

## Building

This contract can be built with truffle. There is an NPM script for doing this
in a way that accounts for the above issues:

```sh
$ npm run prepublish
```

## Development Server

We use `truffle develop` for the development server (which uses ganache under
the hood). The configuration for the development server is located in the
`truffle-config.js` file. The development server is required for the examples to
run and can be started with:

```
$ npm start
```
