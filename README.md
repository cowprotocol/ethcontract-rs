# `ethcontract-rs`

Crate used for generating code for Ethereum smart contracts. It provides a
function procedural macro that generates safe bindings for contract interaction
based on the contract ABI.

## TODO

- [ ] Setup contract events as `futures::future::Stream`
- [ ] Generate documentation based on the truffle artifact `devdocs`
- [ ] Add options for converting ident cases to be more idomatic
- [ ] Add options for creating generic structs (instead of DynTransport)
