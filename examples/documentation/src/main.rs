// This is a binary because when compiling as a library I would get compile errors about the
// artifacts not being found. I suspect this is related to the working directory for tests or doc
// tests being different.

//! Samples of derived contracts for documentation purposes in order to
//! illustrate what the generated API.

ethcontract::contract!("examples/truffle/build/contracts/DocumentedContract.json",);
ethcontract::contract!("examples/truffle/build/contracts/SimpleLibrary.json",);
ethcontract::contract!("examples/truffle/build/contracts/LinkedContract.json",);
ethcontract::contract!("examples/truffle/build/contracts/IERC20.json",);

fn main() {}
