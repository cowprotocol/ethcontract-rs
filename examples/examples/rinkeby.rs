use ethcontract::prelude::*;
use ethcontract::web3::transports::WebSocket;
use std::env;

ethcontract::contract!("examples/truffle/build/contracts/DeployedContract.json");

const RINKEBY_CHAIN_ID: u64 = 4;

#[tokio::main]
async fn main() {
    let account = {
        let pk = env::var("PK").expect("PK is not set");
        let key: PrivateKey = pk.parse().expect("invalid PK");
        Account::Offline(key, Some(RINKEBY_CHAIN_ID))
    };
    let infura_url = {
        let project_id = env::var("INFURA_PROJECT_ID").expect("INFURA_PROJECT_ID is not set");
        format!("wss://rinkeby.infura.io/ws/v3/{}", project_id)
    };

    // NOTE: Use a WebSocket transport for `eth_newBlockFilter` support on
    //   Infura, filters are disabled over HTTPS. Filters are needed for
    //   confirmation support.
    let ws = WebSocket::new(&infura_url).await.expect("transport failed");
    let web3 = Web3::new(ws);

    println!("Account {:?}", account.address());

    let instance = {
        let mut instance = DeployedContract::deployed(&web3)
            .await
            .expect("locating deployed contract failed");
        instance.defaults_mut().from = Some(account);
        instance
    };

    println!(
        "Using contract at {:?} deployed with transaction {:?}",
        instance.address(),
        instance.deployment_information(),
    );

    println!(
        "  value before: {}",
        instance.value().call().await.expect("get value failed")
    );
    println!("  incrementing (this may take a while)...");
    instance
        .increment()
        .confirmations(1) // wait for 1 block confirmation
        .send()
        .await
        .expect("increment failed");
    println!(
        "  value after: {}",
        instance.value().call().await.expect("get value failed")
    );
}
