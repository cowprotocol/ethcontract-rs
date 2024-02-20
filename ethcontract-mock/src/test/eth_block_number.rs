use super::*;

#[tokio::test]
async fn block_number_initially_zero() -> Result {
    let web3 = Mock::new(1234).web3();

    assert_eq!(web3.eth().block_number().await?, 0.into());

    Ok(())
}

#[tokio::test]
async fn block_number_advanced_after_tx() -> Result {
    let (_, web3, contract, instance) = setup();

    contract.expect(ERC20::signatures().transfer());

    assert_eq!(web3.eth().block_number().await?, 0.into());

    instance
        .transfer(address_for("Alice"), 100.into())
        .send()
        .await?;

    assert_eq!(web3.eth().block_number().await?, 1.into());

    Ok(())
}

#[tokio::test]
async fn block_number_advanced_and_confirmed_after_tx() -> Result {
    let (_, web3, contract, instance) = setup();

    contract
        .expect(ERC20::signatures().transfer())
        .confirmations(5);

    assert_eq!(web3.eth().block_number().await?, 0.into());

    instance
        .transfer(address_for("Alice"), 100.into())
        .send()
        .await?;

    assert_eq!(web3.eth().block_number().await?, 6.into());

    Ok(())
}

#[tokio::test]
async fn block_number_is_not_advanced_after_call_or_gas_estimation() -> Result {
    let (_, web3, contract, instance) = setup();

    contract.expect(ERC20::signatures().transfer());

    assert_eq!(web3.eth().block_number().await?, 0.into());

    instance
        .transfer(address_for("Alice"), 100.into())
        .call()
        .await?;

    assert_eq!(web3.eth().block_number().await?, 0.into());

    instance
        .transfer(address_for("Alice"), 100.into())
        .into_inner()
        .estimate_gas()
        .await?;

    assert_eq!(web3.eth().block_number().await?, 0.into());

    Ok(())
}
