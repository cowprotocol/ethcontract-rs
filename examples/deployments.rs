use ethcontract::web3::api::Web3;
use ethcontract::web3::transports::Http;
use futures::compat::Future01CompatExt;

ethcontract::contract!(
    "examples/truffle/build/contracts/RustCoin.json",
    deployments {
        5777 => "0x0123456789012345678901234567890123456789",
    },
);

fn main() {
    futures::executor::block_on(run());
}

async fn run() {
    let (eloop, http) = Http::new("http://localhost:9545").expect("transport failed");
    eloop.into_remote();
    let web3 = Web3::new(http);

    let network_id = web3
        .net()
        .version()
        .compat()
        .await
        .expect("failed to get network ID");
    let instance = RustCoin::deployed(&web3)
        .await
        .expect("faild to find deployment");

    println!(
        "RustCoin deployed on networks {} at {:?}",
        network_id,
        instance.address()
    );
}
