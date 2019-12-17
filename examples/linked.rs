use ethcontract::web3::api::Web3;
use ethcontract::web3::transports::Http;

ethcontract::contract!("examples/truffle/build/contracts/SimpleLibrary.json");
ethcontract::contract!("examples/truffle/build/contracts/LinkedContract.json");

fn main() {
    futures::executor::block_on(run());
}

async fn run() {
    let (eloop, http) = Http::new("http://localhost:9545").expect("transport failure");
    eloop.into_remote();
    let web3 = Web3::new(http);

    let library = SimpleLibrary::builder(&web3)
        .gas(4_712_388.into())
        .confirmations(0)
        .deploy()
        .await
        .expect("library deployment failure");
    let instance = LinkedContract::builder(&web3, library.address(), 1337.into())
        .gas(4_712_388.into())
        .confirmations(0)
        .deploy()
        .await
        .expect("contract deployment failure");

    println!(
        "The value is {}",
        instance.value().call().await.expect("get value failure")
    );
    println!(
        "The answer is {}",
        instance
            .call_answer()
            .call()
            .await
            .expect("callAnswer failure")
    );
}
