use ethcontract_generate::loaders::TruffleLoader;
use ethcontract_generate::ContractBuilder;

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest = std::path::Path::new(&out_dir).join("rust_coin.rs");

    let contract = TruffleLoader::new()
        .load_contract_from_file("../truffle/build/contracts/RustCoin.json")
        .unwrap();
    ContractBuilder::new()
        .generate(&contract)
        .unwrap()
        .write_to_file(dest)
        .unwrap();
}
