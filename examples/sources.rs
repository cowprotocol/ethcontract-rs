use ethcontract::web3::api::Web3;
use ethcontract::web3::transports::Http;
use std::env;

ethcontract::contract!(
    "etherscan:0x60fbbd1fb0076971e8060631b5dd895f55ad5ab7",
    contract = Owl,
);
ethcontract::contract!("npm:@gnosis.pm/owl-token@3.1.0/build/contracts/TokenOWL.json");

fn main() {
    futures::executor::block_on(run());
}

async fn run() {
    let infura_url = {
        let project_id = env::var("INFURA_PROJECT_ID").expect("INFURA_PROJECT_ID is not set");
        format!("https://mainnet.infura.io/v3/{}", project_id)
    };

    let (eloop, http) = Http::new(&infura_url).expect("transport failed");
    eloop.into_remote();
    let web3 = Web3::new(http);

    let instance = Owl::deployed(&web3)
        .await
        .expect("locating deployed contract failed");
    let symbol = instance.symbol().call().await.expect("get symbol failed");

    println!("Etherscan.io ERC20 token {}", symbol);

    let instance = TokenOWL::deployed(&web3)
        .await
        .expect("locating deployed contract failed");
    let symbol = instance.symbol().call().await.expect("get symbol failed");

    println!("npmjs ERC20 token {}", symbol);
}
