use ethcontract_generate::{Address, Builder, TransactionHash};
use std::env;
use std::path::Path;

fn main() {
    let dest = env::var("OUT_DIR").unwrap();
    Builder::new("../truffle/build/contracts/RustCoin.json")
        .add_deployment(42, Address::zero(), Some(TransactionHash::zero().into()))
        .generate()
        .unwrap()
        .write_to_file(Path::new(&dest).join("rust_coin.rs"))
        .unwrap();
}
