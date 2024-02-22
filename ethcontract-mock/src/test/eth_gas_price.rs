use super::*;

#[tokio::test]
async fn gas_price() -> Result {
    let mock = Mock::new(1234);
    let web3 = mock.web3();

    assert_eq!(web3.eth().gas_price().await?, 1.into());

    mock.update_gas_price(10);

    assert_eq!(web3.eth().gas_price().await?, 10.into());

    Ok(())
}
