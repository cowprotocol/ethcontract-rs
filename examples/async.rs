ethcontract::contract!("examples/truffle/build/contracts/RustCoin.json");

#[rustversion::since(1.39)]
fn main() {
    use ethcontract::transaction::Account;
    use ethsign::SecretKey;
    use futures::compat::Future01CompatExt;
    use web3::api::Web3;
    use web3::transports::Http;
    use web3::types::{Address, TransactionRequest, H256};

    async fn print_balance_of(instance: &RustCoin, account: Address) {
        let balance = instance
            .balance_of(account)
            .execute()
            .await
            .expect("balance of");
        println!("Account {:?} has balance of {}", account, balance);
    }

    futures::executor::block_on(async {
        let (eloop, http) = Http::new("http://localhost:7545").expect("transport");
        eloop.into_remote();
        let web3 = Web3::new(http);

        let accounts = web3.eth().accounts().compat().await.expect("get accounts");

        let instance = RustCoin::deploy(&web3)
            .gas(4_712_388.into())
            .confirmations(0)
            .deploy()
            .await
            .expect("deploy");
        let name = instance.name().execute().await.expect("name");
        println!("Deployed {} at {:?}", name, instance.address());

        instance
            .transfer(accounts[1], 1_000_000.into())
            .execute()
            .await
            .expect("transfer 0->1");
        instance
            .transfer(accounts[2], 500_000.into())
            .from(Account::Local(accounts[1], None))
            .execute()
            .await
            .expect("transfer 1->2");

        print_balance_of(&instance, accounts[1]).await;
        print_balance_of(&instance, accounts[2]).await;

        let key = SecretKey::from_raw(
            &"000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
                .parse::<H256>()
                .expect("valid hash")[..],
        )
        .expect("parse key");
        let x: Address = key.public().address().into();
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
            })
            .compat()
            .await
            .expect("send eth");

        instance
            .transfer(x, 1_000_000.into())
            .execute()
            .await
            .expect("transfer 0->x");
        instance
            .transfer(accounts[4], 420.into())
            .from(Account::Offline(key, None))
            .execute()
            .await
            .expect("transfer x->4");

        print_balance_of(&instance, x).await;
        print_balance_of(&instance, accounts[4]).await;
    });
}

#[rustversion::before(1.39)]
fn main() {
    eprintln!("Rust version ^1.39 required for async/await support.");
    std::process::exit(-1);
}
