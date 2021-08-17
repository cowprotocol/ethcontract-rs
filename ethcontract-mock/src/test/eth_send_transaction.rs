use crate::Mock;
use ethcontract::web3::types::TransactionRequest;

#[tokio::test]
#[should_panic(expected = "mock node can't sign transactions")]
async fn send_transaction() {
    // When we implement `send_transaction`, we should add same tests as for
    // send_raw_transaction (expect for raw transaction format/signing)
    // and also a test that checks that returned transaction hash is correct.

    let web3 = Mock::new(1234).web3();

    web3.eth()
        .send_transaction(TransactionRequest::default())
        .await
        .unwrap();
}
