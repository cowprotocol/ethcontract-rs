use ethcontract::web3::api::Web3;
use ethcontract::web3::transports::Http;
use ethcontract::{Address, Topic, U256};
use futures::compat::Future01CompatExt;
use futures::join;
use futures::stream::StreamExt;

ethcontract::contract!("examples/truffle/build/contracts/RustCoin.json");

fn main() {
    futures::executor::block_on(run());
}

async fn run() {
    let (eloop, http) = Http::new("http://localhost:9545").expect("transport failed");
    eloop.into_remote();
    let web3 = Web3::new(http);

    let accounts = web3
        .eth()
        .accounts()
        .compat()
        .await
        .expect("get accounts failed");

    let instance = RustCoin::builder(&web3)
        .gas(4_712_388.into())
        .deploy()
        .await
        .expect("deployment failed");
    let mut transfers = instance
        .instance
        .event::<_, (Address, Address, U256)>("Transfer")
        .expect("transfer event not found")
        .topic0(Topic::This(accounts[0])) // from
        .stream()
        .expect("failed to encode topic filters");

    join! {
        async {
            instance
                .transfer(accounts[1], 1_000_000.into())
                .send()
                .await
                .expect("transfer 0->1 failed");
        },
        async {
            let (_, to, amount) = transfers.next()
                .await
                .expect("no more events")
                .expect("failed to get event")
                .added()
                .expect("expected added event");
            println!("Received a transfer event to {:?} with amount {}", to, amount);
        },
    };
}
