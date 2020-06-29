use ethcontract::prelude::*;

ethcontract::contract!(
    "examples/truffle/build/contracts/OverloadedMethods.json",
    methods {
        getValue(bool) as get_bool_value;
    },
);

#[tokio::main]
async fn main() {
    let http = Http::new("http://localhost:9545").expect("transport failure");
    let web3 = Web3::new(http);

    let instance = OverloadedMethods::builder(&web3)
        .gas(4_712_388.into())
        .deploy()
        .await
        .expect("contract deployment failure");
    println!("Using contract at {:?}", instance.address());

    println!(
        "U256 value: {}",
        instance
            .get_value(84.into())
            .call()
            .await
            .expect("get value failed"),
    );
    println!(
        "bool value: {}",
        instance
            .get_bool_value(false)
            .call()
            .await
            .expect("get value failed"),
    );
}
