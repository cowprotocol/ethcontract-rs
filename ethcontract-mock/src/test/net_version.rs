use super::*;
use crate::Mock;

#[tokio::test]
async fn chain_id() -> Result {
    let web3 = Mock::new(1234).web3();

    assert_eq!(web3.eth().chain_id().await?, 1234.into());

    Ok(())
}
