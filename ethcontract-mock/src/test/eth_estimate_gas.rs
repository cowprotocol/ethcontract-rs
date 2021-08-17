use super::*;
use ethcontract::web3::types::CallRequest;

#[tokio::test]
async fn estimate_gas_returns_one() -> Result {
    let (_, _, contract, instance) = setup();

    contract.expect(IERC20::signatures().transfer());

    let gas = instance
        .transfer(address_for("Alice"), 100.into())
        .into_inner()
        .estimate_gas()
        .await?;

    assert_eq!(gas, 1.into());

    Ok(())
}

#[tokio::test]
async fn estimate_gas_is_supported_for_edge_block() -> Result {
    let (_, web3, contract, instance) = setup();

    contract.expect(IERC20::signatures().transfer());

    instance
        .transfer(address_for("Bob"), 100.into())
        .send()
        .await?;
    instance
        .transfer(address_for("Bob"), 100.into())
        .send()
        .await?;

    let request = {
        let tx = instance
            .transfer(address_for("Alice"), 100.into())
            .into_inner();

        CallRequest {
            from: Some(address_for("Alice")),
            to: Some(contract.address),
            gas: None,
            gas_price: None,
            value: None,
            data: tx.data,
            transaction_type: None,
            access_list: None,
        }
    };

    assert_eq!(
        web3.eth()
            .estimate_gas(request.clone(), Some(BlockNumber::Latest))
            .await?,
        1.into()
    );
    assert_eq!(
        web3.eth()
            .estimate_gas(request.clone(), Some(BlockNumber::Pending))
            .await?,
        1.into()
    );
    assert_eq!(
        web3.eth()
            .estimate_gas(request.clone(), Some(BlockNumber::Number(2.into())))
            .await?,
        1.into()
    );

    Ok(())
}

#[tokio::test]
#[should_panic(expected = "mock node does not support executing methods on non-last block")]
async fn estimate_gas_is_not_supported_for_custom_block() {
    let (_, web3, contract, instance) = setup();

    contract.expect(IERC20::signatures().transfer());

    let request = {
        let tx = instance
            .transfer(address_for("Alice"), 100.into())
            .into_inner();

        CallRequest {
            from: Some(address_for("Alice")),
            to: Some(contract.address),
            gas: None,
            gas_price: None,
            value: None,
            data: tx.data,
            transaction_type: None,
            access_list: None,
        }
    };

    web3.eth()
        .estimate_gas(request.clone(), Some(BlockNumber::Number(1.into())))
        .await
        .unwrap();
}

#[tokio::test]
#[should_panic(expected = "mock node does not support executing methods on earliest block")]
async fn estimate_gas_is_not_supported_for_earliest_block() {
    let (_, web3, contract, instance) = setup();

    contract.expect(IERC20::signatures().transfer());

    let request = {
        let tx = instance
            .transfer(address_for("Alice"), 100.into())
            .into_inner();

        CallRequest {
            from: Some(address_for("Alice")),
            to: Some(contract.address),
            gas: None,
            gas_price: None,
            value: None,
            data: tx.data,
            transaction_type: None,
            access_list: None,
        }
    };

    web3.eth()
        .estimate_gas(request.clone(), Some(BlockNumber::Earliest))
        .await
        .unwrap();
}
