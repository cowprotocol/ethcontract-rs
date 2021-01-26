use ethcontract::{batch::CallBatch, prelude::*};

ethcontract::contract!("examples/truffle/build/contracts/RustCoin.json");

#[tokio::main]
async fn main() {
    let http = Http::new("http://localhost:9545").expect("transport failed");
    let web3 = Web3::new(http);

    let accounts = web3.eth().accounts().await.expect("get accounts failed");

    let instance = RustCoin::builder(&web3)
        .gas(4_712_388u64.into())
        .deploy()
        .await
        .expect("deployment failed");
    let name = instance.name().call().await.expect("get name failed");
    println!("Deployed {} at {:?}", name, instance.address());

    instance
        .transfer(accounts[1], 1_000_000u64.into())
        .send()
        .await
        .expect("transfer 0->1 failed");
    instance
        .transfer(accounts[2], 500_000u64.into())
        .send()
        .await
        .expect("transfer 1->2 failed");

    let mut batch = CallBatch::new(web3.transport());
    let calls = vec![
        instance
            .balance_of(accounts[1])
            .view()
            .batch_call(&mut batch),
        instance
            .balance_of(accounts[2])
            .view()
            .batch_call(&mut batch),
    ];
    batch.execute_all().await.unwrap();
    for (id, call) in calls.into_iter().enumerate() {
        println!("Call {} returned {}", id, call.await.unwrap());
    }
}
