use ethcontract_generate::loaders::HardHatLoader;
use ethcontract_generate::ContractBuilder;

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest = std::path::Path::new(&out_dir).join("contracts.rs");

    let artifact = HardHatLoader::new()
        .deny_by_name("localhost")
        .load_from_directory("../hardhat/deployments")
        .unwrap();

    for contract in artifact.iter() {
        ContractBuilder::new()
            .generate(contract)
            .unwrap()
            .write_to_file(&dest)
            .unwrap();
    }
}
