ethcontract::contract!("examples/truffle/build/contracts/RustCoin.json");

#[rustversion::since(1.39)]
fn main() {
    use ethcontract::transaction::Account;
    use ethsign::SecretKey;
    use futures::compat::Future01CompatExt;
    use web3::api::Web3;
    use web3::transports::Http;
    use web3::types::{Address, H256};

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
            &"c9a3c7d0f7685dc5e08cc990a4614666202258db9ca93e429499c3884a07782a"
                .parse::<H256>()
                .expect("valid hash")[..],
        )
        .expect("parse key");
        assert_eq!(accounts[3], key.public().address().into());

        instance
            .transfer(accounts[3], 1_000_000.into())
            .execute()
            .await
            .expect("transfer 0->3");
        instance
            .transfer(accounts[4], 420.into())
            .from(Account::Offline(key, None))
            .execute()
            .await
            .expect("transfer 3->4");

        print_balance_of(&instance, accounts[3]).await;
        print_balance_of(&instance, accounts[4]).await;
    });
}

#[rustversion::before(1.39)]
fn main() {
    eprintln!("Rust version ^1.39 required for async/await support.");
    std::process::exit(-1);
}
