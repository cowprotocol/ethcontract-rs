use super::*;
use crate::Mock;

#[tokio::test]
async fn chain_id() -> Result {
    let web3 = Mock::new(1234).web3();

    assert_eq!(web3.eth().chain_id().await?, 1234.into());

    Ok(())
}

#[tokio::test]
async fn net_version() -> Result {
    let web3 = Mock::new(1234).web3();

    assert_eq!(web3.net().version().await?, "1234");

    Ok(())
}

#[tokio::test]
async fn net_version_main() -> Result {
    let web3 = Mock::new(1).web3(); // simulate mainnet

    assert_eq!(web3.net().version().await?, "1");

    Ok(())
}
