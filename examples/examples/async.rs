use ethcontract::prelude::*;
use ethcontract::web3::types::TransactionRequest;

ethcontract::contract!("examples/truffle/build/contracts/RustCoin.json");

#[tokio::main]
async fn main() {
    let http = Http::new("http://localhost:9545").expect("transport failed");
    let web3 = Web3::new(http);

    let accounts = web3.eth().accounts().await.expect("get accounts failed");

    let instance = RustCoin::builder(&web3)
        .gas(4_712_388.into())
        .deploy()
        .await
        .expect("deployment failed");
    let name = instance.name().call().await.expect("get name failed");
    println!("Deployed {} at {:?}", name, instance.address());

    instance
        .transfer(accounts[1], 1_000_000.into())
        .send()
        .await
        .expect("transfer 0->1 failed");
    instance
        .transfer(accounts[2], 500_000.into())
        .from(Account::Local(accounts[1], None))
        .send()
        .await
        .expect("transfer 1->2 failed");

    print_balance_of(&instance, accounts[1]).await;
    print_balance_of(&instance, accounts[2]).await;

    let key: PrivateKey = "0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
        .parse()
        .expect("parse key");
    let x = key.public_address();
    println!("Created new account {:?}", x);

    // send some eth to x so that it can do transactions
    web3.eth()
        .send_transaction(TransactionRequest {
            from: accounts[0],
            to: Some(x),
            gas: None,
            gas_price: None,
            value: Some(1_000_000_000_000_000_000u64.into()),
            data: None,
            nonce: None,
            condition: None,
            transaction_type: None,
            access_list: None,
        })
        .await
        .expect("send eth failed");

    instance
        .transfer(x, 1_000_000.into())
        .send()
        .await
        .expect("transfer 0->x failed");
    instance
        .transfer(accounts[4], 420.into())
        .from(Account::Offline(key, None))
        .send()
        .await
        .expect("transfer x->4 failed");

    print_balance_of(&instance, x).await;
    print_balance_of(&instance, accounts[4]).await;

    // mint some RustCoin with the fallback method
    instance
        .fallback(vec![])
        .from(Account::Local(accounts[3], None))
        .value(1_000_000_000_000_000_000u64.into())
        .send()
        .await
        .expect("mint 3 failed");
    print_balance_of(&instance, accounts[3]).await;
}

async fn print_balance_of(instance: &RustCoin, account: Address) {
    let balance = instance
        .balance_of(account)
        .call()
        .await
        .expect("balance of failed");
    println!("Account {:?} has balance of {}", account, balance);
}
