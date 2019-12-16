use ethcontract::transaction::Account;
use ethsign::SecretKey;
use std::env;
use std::time::Duration;
use web3::api::Web3;
use web3::transports::WebSocket;
use web3::types::H256;

ethcontract::contract!("examples/truffle/build/contracts/DeployedContract.json");

const RINKEBY_CHAIN_ID: u64 = 4;

fn main() {
    futures::executor::block_on(run());
}

async fn run() {
    let account = {
        let pk = env::var("PK").expect("PK is not set");
        let raw_key: H256 = pk.parse().expect("invalid PK");
        let key = SecretKey::from_raw(&raw_key[..]).expect("invalid PK");
        Account::Offline(key, Some(RINKEBY_CHAIN_ID))
    };
    let infura_url = {
        let project_id = env::var("INFURA_PROJECT_ID").expect("INFURA_PROJECT_ID is not set");
        format!("wss://rinkeby.infura.io/ws/v3/{}", project_id)
    };

    // use a WebSocket transport to support confirmations
    let (eloop, ws) = WebSocket::new(&infura_url).expect("transport failed");
    eloop.into_remote();
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
        "  value before: {}",
        instance
            .value()
            .call()
            .await
            .expect("get value failed")
    );
    println!("  incrementing (this may take a while)...");
    instance
        .increment()
        .send_and_confirm(Duration::new(5, 0), 1)
        .await
        .expect("increment failed");
    println!(
        "  value after: {}",
        instance
            .value()
            .call()
            .await
            .expect("get value failed")
    );
}
