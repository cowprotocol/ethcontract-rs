use super::*;
use ethcontract::transaction::ResolveCondition;

#[tokio::test]
async fn transaction_receipt_is_returned() -> Result {
    let (_, web3, contract, instance) = setup();

    contract.expect(ERC20::signatures().transfer());

    let hash = instance
        .transfer(address_for("Bob"), 100.into())
        .into_inner()
        .resolve(ResolveCondition::Pending)
        .send()
        .await?
        .hash();

    let receipt = web3.eth().transaction_receipt(hash).await?.unwrap();
    assert_eq!(receipt.transaction_hash, hash);
    assert_eq!(receipt.block_number, Some(1.into()));
    assert_eq!(receipt.status, Some(1.into()));

    Ok(())
}

#[tokio::test]
#[should_panic(expected = "there is no transaction with hash")]
async fn transaction_receipt_is_panicking_when_hash_not_fount() {
    let web3 = Mock::new(1234).web3();

    web3.eth()
        .transaction_receipt(Default::default())
        .await
        .unwrap();
}
