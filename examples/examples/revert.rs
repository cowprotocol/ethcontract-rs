use ethcontract::{errors::ExecutionError, prelude::*};

ethcontract::contract!("examples/truffle/build/contracts/Revert.json");

// Can use this to test with infura.
async fn _contract_ropsten() -> Revert {
    let http = Http::new("https://ropsten.infura.io/v3/f27cfd9cca1a41d2a56ca5df0e9bff5e").unwrap();
    let address: H160 = "0x2B8d1E12c4e87cEedf8B1DcA133983d6493Ff780"
        .parse()
        .unwrap();
    let web3 = Web3::new(http);
    Revert::at(&web3, address)
}

async fn contract_ganache() -> Revert {
    let http = Http::new("http://localhost:9545").unwrap();
    let web3 = Web3::new(http);
    Revert::builder(&web3).deploy().await.unwrap()
}

#[tokio::main]
async fn main() {
    let instance = contract_ganache().await;

    let result_0 = dbg!(instance.revert_with_reason().call().await);
    let result_1 = dbg!(instance.revert_without_reason().call().await);
    let result_2 = dbg!(instance.invalid_op_code().call().await);

    let error = result_0.unwrap_err().inner;
    assert!(matches!(
        error,
        ExecutionError::Revert(Some(reason)) if reason == "reason"
    ));

    let error = result_1.unwrap_err().inner;
    assert!(matches!(error, ExecutionError::Revert(None)));

    let error = result_2.unwrap_err().inner;
    assert!(matches!(error, ExecutionError::InvalidOpcode));
}
