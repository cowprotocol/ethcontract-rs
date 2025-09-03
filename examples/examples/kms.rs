use ethcontract::{
    aws_config,
    prelude::*,
    transaction::{kms, TransactionBuilder},
};
use std::env;

#[tokio::main]
async fn main() {
    // Run `aws configure export-credentials --profile cow-staging --format env` to get required env variable locally
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let account = kms::Account::new(
        (&config).into(),
        &env::var("KMS_KEY_ID").expect("KMS_KEY_ID not set"),
    )
    .await
    .unwrap();

    let web3 = {
        let url = env::var("NODE_URL").expect("NODE_URL not set");
        let http = Http::new(&url).expect("transport failed");
        Web3::new(http)
    };
    println!(
        "Sending transaction to self: {:?}",
        account.public_address()
    );

    let chain_id = web3.eth().chain_id().await.expect("Failed to get chainID");
    let receipt = TransactionBuilder::new(web3)
        .from(Account::Kms(account.clone(), Some(chain_id.as_u64())))
        .to(account.public_address())
        .send()
        .await
        .unwrap();
    println!("Transaction hash: {:?}", receipt.hash());
}
