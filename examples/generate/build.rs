use ethcontract_generate::Builder;
use std::env;
use std::path::Path;

fn main() {
    let dest = env::var("OUT_DIR").unwrap();
    Builder::new("../truffle/build/contracts/RustCoin.json")
        .generate()
        .unwrap()
        .write_to_file(Path::new(&dest).join("rust_coin.rs"))
        .unwrap();
}
