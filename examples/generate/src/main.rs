use ethcontract::prelude::*;

include!(concat!(env!("OUT_DIR"), "/rust_coin.rs"));

fn main() {
    futures::executor::block_on(run());
}

async fn run() {
    let (eloop, http) = Http::new("http://localhost:9545").expect("create transport failed");
    eloop.into_remote();
    let web3 = Web3::new(http);

    let instance = RustCoin::builder(&web3)
        .gas(4_712_388.into())
        .deploy()
        .await
        .expect("deployment failed");

    println!(
        "using {} ({}) at {:?}",
        instance.name().call().await.expect("get name failed"),
        instance.symbol().call().await.expect("get name failed"),
        instance.address()
    );
}
