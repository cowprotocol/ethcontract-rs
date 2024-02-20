use super::*;

#[tokio::test]
async fn transaction_count_initially_zero() -> Result {
    let web3 = Mock::new(1234).web3();

    assert_eq!(
        web3.eth()
            .transaction_count(address_for("Alice"), None)
            .await?,
        0.into()
    );

    Ok(())
}

#[tokio::test]
async fn transaction_count_advanced_after_tx() -> Result {
    let (_, web3, contract, instance) = setup();

    contract.expect(ERC20::signatures().transfer());

    assert_eq!(
        web3.eth()
            .transaction_count(address_for("Alice"), None)
            .await?,
        0.into()
    );

    instance
        .transfer(address_for("Bob"), 100.into())
        .send()
        .await?;

    assert_eq!(
        web3.eth()
            .transaction_count(address_for("Alice"), None)
            .await?,
        1.into()
    );

    Ok(())
}

#[tokio::test]
async fn transaction_count_is_not_advanced_after_call_or_gas_estimation() -> Result {
    let (_, web3, contract, instance) = setup();

    contract.expect(ERC20::signatures().transfer());

    assert_eq!(
        web3.eth()
            .transaction_count(address_for("Alice"), None)
            .await?,
        0.into()
    );

    instance
        .transfer(address_for("Bob"), 100.into())
        .call()
        .await?;

    assert_eq!(
        web3.eth()
            .transaction_count(address_for("Alice"), None)
            .await?,
        0.into()
    );

    instance
        .transfer(address_for("Bob"), 100.into())
        .into_inner()
        .estimate_gas()
        .await?;

    assert_eq!(
        web3.eth()
            .transaction_count(address_for("Alice"), None)
            .await?,
        0.into()
    );

    Ok(())
}

#[tokio::test]
async fn transaction_count_is_supported_for_edge_block() -> Result {
    let (_, web3, contract, instance) = setup();

    contract.expect(ERC20::signatures().transfer());

    instance
        .transfer(address_for("Bob"), 100.into())
        .send()
        .await?;
    instance
        .transfer(address_for("Bob"), 100.into())
        .send()
        .await?;

    assert_eq!(
        web3.eth()
            .transaction_count(address_for("Alice"), Some(BlockNumber::Earliest))
            .await?,
        0.into()
    );
    assert_eq!(
        web3.eth()
            .transaction_count(address_for("Alice"), Some(BlockNumber::Number(0.into())))
            .await?,
        0.into()
    );

    assert_eq!(
        web3.eth()
            .transaction_count(address_for("Alice"), Some(BlockNumber::Latest))
            .await?,
        2.into()
    );
    assert_eq!(
        web3.eth()
            .transaction_count(address_for("Alice"), Some(BlockNumber::Pending))
            .await?,
        2.into()
    );
    assert_eq!(
        web3.eth()
            .transaction_count(address_for("Alice"), Some(BlockNumber::Number(2.into())))
            .await?,
        2.into()
    );

    Ok(())
}

#[tokio::test]
#[should_panic(
    expected = "mock node does not support returning transaction count for specific block number"
)]
async fn transaction_count_is_not_supported_for_custom_block() {
    let web3 = Mock::new(1234).web3();

    web3.eth()
        .transaction_count(address_for("Alice"), Some(BlockNumber::Number(1.into())))
        .await
        .unwrap();
}
