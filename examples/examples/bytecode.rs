use ethcontract::prelude::*;

ethcontract::contract!("examples/truffle/build/contracts/RustCoin.json");

#[tokio::main]
async fn main() {
    let http = Http::new("http://localhost:9545").expect("transport failed");
    let web3 = Web3::new(http);

    let instance = RustCoin::builder(&web3)
        .gas(4_712_388.into())
        .deploy()
        .await
        .expect("deployment failed");

    let code = web3
        .eth()
        .code(instance.address(), None)
        .await
        .expect("get code failed");
    assert_eq!(
        code,
        RustCoin::raw_contract()
            .deployed_bytecode
            .to_bytes()
            .expect("failed to read contract deployed bytecode"),
    );

    println!("RustCoin deployment matches expected bytecode");
}
