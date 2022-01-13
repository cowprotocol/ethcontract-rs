use super::*;

#[tokio::test]
async fn batch_ok() -> Result {
    let (_, _, contract, instance) = setup();

    let mut seq = mockall::Sequence::new();

    let contract = contract
        .expect(ERC20::signatures().name())
        .once()
        .in_sequence(&mut seq)
        .returns("WrappedEther".into());
    let contract = contract
        .expect(ERC20::signatures().symbol())
        .once()
        .in_sequence(&mut seq)
        .returns("WETH".into());
    let contract = contract
        .expect(ERC20::signatures().decimals())
        .once()
        .returns(18)
        .in_sequence(&mut seq);
    let contract = contract
        .expect(ERC20::signatures().total_supply())
        .once()
        .in_sequence(&mut seq)
        .returns_error("failed calculating total supply".into());

    let mut batch = ethcontract::batch::CallBatch::new(contract.transport());

    let name = instance.name().batch_call(&mut batch);
    let symbol = instance.symbol().batch_call(&mut batch);
    let decimals = instance.decimals().batch_call(&mut batch);
    let total_supply = instance.total_supply().batch_call(&mut batch);

    batch.execute_all(4).await;

    assert_eq!(name.await?, "WrappedEther");
    assert_eq!(symbol.await?, "WETH");
    assert_eq!(decimals.await?, 18);
    assert!(total_supply
        .await
        .unwrap_err()
        .to_string()
        .contains("failed calculating total supply"));

    Ok(())
}
