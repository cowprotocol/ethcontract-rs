use super::*;

ethcontract::contract!("examples/truffle/build/contracts/AbiTypes.json");

#[tokio::test]
async fn returns_default() -> Result {
    let contract = Mock::new(1234).deploy(AbiTypes::raw_contract().interface.abi.clone());

    contract.expect(AbiTypes::signatures().get_void());
    contract.expect(AbiTypes::signatures().get_u8());
    contract.expect(AbiTypes::signatures().abiv_2_struct());
    contract.expect(AbiTypes::signatures().abiv_2_array_of_struct());
    contract.expect(AbiTypes::signatures().multiple_results());
    contract.expect(AbiTypes::signatures().multiple_results_struct());

    let instance = AbiTypes::at(&contract.web3(), contract.address);

    instance.get_void().call().await?;
    assert_eq!(instance.get_u8().call().await?, 0);
    assert_eq!(instance.abiv_2_struct((1, 2)).call().await?, (0, 0));
    assert_eq!(
        instance
            .abiv_2_array_of_struct(vec![(1, 2), (3, 4)])
            .call()
            .await?,
        vec![]
    );
    assert_eq!(instance.multiple_results().call().await?, (0, 0, 0));
    assert_eq!(
        instance.multiple_results_struct().call().await?,
        ((0, 0), (0, 0))
    );

    Ok(())
}

#[tokio::test]
async fn returns_const() -> Result {
    let contract = Mock::new(1234).deploy(AbiTypes::raw_contract().interface.abi.clone());

    contract
        .expect(AbiTypes::signatures().get_void())
        .returns(());
    contract.expect(AbiTypes::signatures().get_u8()).returns(42);
    contract
        .expect(AbiTypes::signatures().abiv_2_struct())
        .returns((1, 2));
    contract
        .expect(AbiTypes::signatures().abiv_2_array_of_struct())
        .returns(vec![(1, 2), (3, 4)]);
    contract
        .expect(AbiTypes::signatures().multiple_results())
        .returns((1, 2, 3));
    contract
        .expect(AbiTypes::signatures().multiple_results_struct())
        .returns(((1, 2), (3, 4)));

    let instance = AbiTypes::at(&contract.web3(), contract.address);

    instance.get_void().call().await?;
    assert_eq!(instance.get_u8().call().await?, 42);
    assert_eq!(instance.abiv_2_struct((1, 2)).call().await?, (1, 2));
    assert_eq!(
        instance
            .abiv_2_array_of_struct(vec![(1, 2), (3, 4)])
            .call()
            .await?,
        vec![(1, 2), (3, 4)]
    );
    assert_eq!(instance.multiple_results().call().await?, (1, 2, 3));
    assert_eq!(
        instance.multiple_results_struct().call().await?,
        ((1, 2), (3, 4))
    );

    Ok(())
}

#[tokio::test]
async fn returns_fn() -> Result {
    let contract = Mock::new(1234).deploy(AbiTypes::raw_contract().interface.abi.clone());

    contract
        .expect(AbiTypes::signatures().get_void())
        .returns_fn(|_| Ok(()));
    contract
        .expect(AbiTypes::signatures().get_u8())
        .returns_fn(|_| Ok(42));
    contract
        .expect(AbiTypes::signatures().abiv_2_struct())
        .returns_fn(|(x,)| Ok(x));
    contract
        .expect(AbiTypes::signatures().abiv_2_array_of_struct())
        .returns_fn(|(x,)| Ok(x));
    contract
        .expect(AbiTypes::signatures().multiple_results())
        .returns_fn(|_| Ok((1, 2, 3)));
    contract
        .expect(AbiTypes::signatures().multiple_results_struct())
        .returns_fn(|_| Ok(((1, 2), (3, 4))));

    let instance = AbiTypes::at(&contract.web3(), contract.address);

    instance.get_void().call().await?;
    assert_eq!(instance.get_u8().call().await?, 42);
    assert_eq!(instance.abiv_2_struct((1, 2)).call().await?, (1, 2));
    assert_eq!(
        instance
            .abiv_2_array_of_struct(vec![(1, 2), (3, 4)])
            .call()
            .await?,
        vec![(1, 2), (3, 4)]
    );
    assert_eq!(instance.multiple_results().call().await?, (1, 2, 3));
    assert_eq!(
        instance.multiple_results_struct().call().await?,
        ((1, 2), (3, 4))
    );

    Ok(())
}

#[tokio::test]
async fn returns_fn_ctx() -> Result {
    let contract = Mock::new(1234).deploy(AbiTypes::raw_contract().interface.abi.clone());

    contract
        .expect(AbiTypes::signatures().get_void())
        .returns_fn_ctx(|_, _| Ok(()));
    contract
        .expect(AbiTypes::signatures().get_u8())
        .returns_fn_ctx(|_, _| Ok(42));
    contract
        .expect(AbiTypes::signatures().abiv_2_struct())
        .returns_fn_ctx(|_, (x,)| Ok(x));
    contract
        .expect(AbiTypes::signatures().abiv_2_array_of_struct())
        .returns_fn_ctx(|_, (x,)| Ok(x));
    contract
        .expect(AbiTypes::signatures().multiple_results())
        .returns_fn_ctx(|_, _| Ok((1, 2, 3)));
    contract
        .expect(AbiTypes::signatures().multiple_results_struct())
        .returns_fn_ctx(|_, _| Ok(((1, 2), (3, 4))));

    let instance = AbiTypes::at(&contract.web3(), contract.address);

    instance.get_void().call().await?;
    assert_eq!(instance.get_u8().call().await?, 42);
    assert_eq!(instance.abiv_2_struct((1, 2)).call().await?, (1, 2));
    assert_eq!(
        instance
            .abiv_2_array_of_struct(vec![(1, 2), (3, 4)])
            .call()
            .await?,
        vec![(1, 2), (3, 4)]
    );
    assert_eq!(instance.multiple_results().call().await?, (1, 2, 3));
    assert_eq!(
        instance.multiple_results_struct().call().await?,
        ((1, 2), (3, 4))
    );

    Ok(())
}
