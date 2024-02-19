use ethcontract::prelude::*;

ethcontract::contract!(
    "examples/truffle/build/contracts/RustCoin.json",
    deployments {
        31337 => "0x0123456789012345678901234567890123456789",
    },
);

#[tokio::main]
async fn main() {
    let http = Http::new("http://localhost:9545").expect("transport failed");
    let web3 = Web3::new(http);

    let network_id = web3
        .eth()
        .chain_id()
        .await
        .expect("failed to get network ID");
    let instance = RustCoin::deployed(&web3)
        .await
        .expect("failed to find deployment");

    println!(
        "RustCoin deployed on networks {} at {:?}",
        network_id,
        instance.address()
    );
}
