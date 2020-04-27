use ethcontract::prelude::*;
use std::env;

ethcontract::contract!("npm:@gnosis.pm/owl-token@3.1.0/build/contracts/TokenOWLProxy.json");
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

    let owl_proxy = TokenOWLProxy::deployed(&web3)
        .await
        .expect("locating deployed contract failed");

    // Casting proxy token into actual token
    let owl_token =
        TokenOWL::with_transaction(&web3, owl_proxy.address(), owl_proxy.transaction_hash());
    println!("Using OWL token at {:?}", owl_token.address());
    println!("Retrieving all past events (this could take a while)...");
    let event_history = owl_token
        .all_events()
        .query_past_events_paginated()
        .await
        .expect("Couldn't retrieve event history");
    println!(
        "Total number of events emitted by OWL token {:}",
        event_history.len()
    );
}
