#[allow(warnings, unused)]
mod contract {
    include!(concat!(env!("OUT_DIR"), "/rust_coin.rs"));
}

use crate::contract::RustCoin;
use web3::api::Web3;
use web3::transports::Http;

fn main() {
    futures::executor::block_on(run());
}

async fn run() {
    let (eloop, http) = Http::new("http://localhost:9545").expect("create transport failed");
    eloop.into_remote();
    let web3 = Web3::new(http);

    let instance = RustCoin::deploy(&web3)
        .gas(4_712_388.into())
        .confirmations(0)
        .deploy()
        .await
        .expect("deployment failed");

    println!(
        "using {} ({}) at {:?}:",
        instance.name().call().await.expect("get name failed"),
        instance.symbol().call().await.expect("get name failed"),
        instance.address()
    );
}
