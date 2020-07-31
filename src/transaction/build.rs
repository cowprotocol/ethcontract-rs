//! This module implements transaction finalization from partial transaction
//! parameters. It provides futures for building `TransactionRequest` instances
//! and raw `Bytes` transactions from partial transaction parameters, where the
//! remaining parameters are queried from the node before finalizing the
//! transaction.

use crate::errors::ExecutionError;
use crate::secret::{Password, PrivateKey};
use crate::transaction::gas_price::GasPrice;
use crate::transaction::{Account, TransactionBuilder};
use web3::api::Web3;
use web3::signing::SecretKeyRef;
use web3::types::{
    Address, Bytes, CallRequest, TransactionCondition, TransactionParameters, TransactionRequest,
    U256,
};
use web3::Transport;

impl<T: Transport> TransactionBuilder<T> {
    /// Build a prepared transaction that is ready to send.
    ///
    /// Can resolve into either a `TransactionRequest` for sending locally
    /// signed transactions or raw signed transaction `Bytes` when sending a raw
    /// transaction.
    pub async fn build(self) -> Result<Transaction, ExecutionError> {
        let gas_price = self.gas_price.unwrap_or_default();
        let options = TransactionOptions {
            to: self.to,
            gas: self.gas,
            value: self.value,
            data: self.data,
            nonce: self.nonce,
        };

        let tx = match self.from {
            None => Transaction::Request(
                build_transaction_request_for_local_signing(
                    self.web3,
                    None,
                    gas_price,
                    TransactionRequestOptions(options, None),
                )
                .await?,
            ),
            Some(Account::Local(from, condition)) => Transaction::Request(
                build_transaction_request_for_local_signing(
                    self.web3,
                    Some(from),
                    gas_price,
                    TransactionRequestOptions(options, condition),
                )
                .await?,
            ),
            Some(Account::Locked(from, password, condition)) => Transaction::Raw(
                build_transaction_signed_with_locked_account(
                    self.web3,
                    from,
                    password,
                    gas_price,
                    TransactionRequestOptions(options, condition),
                )
                .await?,
            ),
            Some(Account::Offline(key, chain_id)) => Transaction::Raw(
                build_offline_signed_transaction(self.web3, key, chain_id, gas_price, options)
                    .await?,
            ),
        };

        Ok(tx)
    }
}

/// Represents a prepared and optionally signed transaction that is ready for
/// sending created by a `TransactionBuilder`.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Transaction {
    /// A structured transaction request to be signed locally by the node.
    Request(TransactionRequest),
    /// A signed raw transaction request.
    Raw(Bytes),
}

impl Transaction {
    /// Unwraps the transaction into a transaction request, returning None if the
    /// transaction is a raw transaction.
    pub fn request(self) -> Option<TransactionRequest> {
        match self {
            Transaction::Request(tx) => Some(tx),
            _ => None,
        }
    }

    /// Unwraps the transaction into its raw bytes, returning None if it is a
    /// transaction request.
    pub fn raw(self) -> Option<Bytes> {
        match self {
            Transaction::Raw(tx) => Some(tx),
            _ => None,
        }
    }
}

/// Shared transaction options that are used when finalizing transactions into
/// either `TransactionRequest`s or raw signed transaction `Bytes`.
#[derive(Clone, Debug, Default)]
struct TransactionOptions {
    /// The receiver of the transaction.
    pub to: Option<Address>,
    /// The amount of gas to use for the transaction.
    pub gas: Option<U256>,
    /// The ETH value to send with the transaction.
    pub value: Option<U256>,
    /// The data for the transaction.
    pub data: Option<Bytes>,
    /// The transaction nonce.
    pub nonce: Option<U256>,
}

/// Transaction options specific to `TransactionRequests` since they may also
/// include a `TransactionCondition` that is not applicable to raw signed
/// transactions.
#[derive(Clone, Debug, Default)]
struct TransactionRequestOptions(TransactionOptions, Option<TransactionCondition>);

impl TransactionRequestOptions {
    /// Builds a `TransactionRequest` from a `TransactionRequestOptions` by
    /// specifying the missing parameters.
    fn build_request(self, from: Address, gas_price: Option<U256>) -> TransactionRequest {
        TransactionRequest {
            from,
            to: self.0.to,
            gas: self.0.gas,
            gas_price,
            value: self.0.value,
            data: self.0.data,
            nonce: self.0.nonce,
            condition: self.1,
        }
    }
}

/// Build a transaction request to locally signed by the node before sending.
async fn build_transaction_request_for_local_signing<T: Transport>(
    web3: Web3<T>,
    from: Option<Address>,
    gas_price: GasPrice,
    options: TransactionRequestOptions,
) -> Result<TransactionRequest, ExecutionError> {
    let from = match from {
        Some(address) => address,
        None => *web3
            .eth()
            .accounts()
            .await?
            .get(0)
            .ok_or(ExecutionError::NoLocalAccounts)?,
    };
    let gas_price = gas_price.resolve_for_transaction_request(&web3).await?;

    let request = options.build_request(from, gas_price);

    Ok(request)
}

/// Build a locally signed transaction with a locked account.
async fn build_transaction_signed_with_locked_account<T: Transport>(
    web3: Web3<T>,
    from: Address,
    password: Password,
    gas_price: GasPrice,
    options: TransactionRequestOptions,
) -> Result<Bytes, ExecutionError> {
    let gas_price = gas_price.resolve_for_transaction_request(&web3).await?;

    let request = options.build_request(from, gas_price);
    let signed_tx = web3.personal().sign_transaction(request, &password).await?;

    Ok(signed_tx.raw)
}

/// Build an offline signed transaction.
///
/// Note that all transaction parameters must be finalized before signing. This
/// means that things like account nonce, gas and gas price estimates, as well
/// as chain ID must be queried from the node if not provided before signing.
async fn build_offline_signed_transaction<T: Transport>(
    web3: Web3<T>,
    key: PrivateKey,
    chain_id: Option<u64>,
    gas_price: GasPrice,
    options: TransactionOptions,
) -> Result<Bytes, ExecutionError> {
    let gas = match options.gas {
        Some(value) => value,
        None => {
            web3.eth()
                .estimate_gas(
                    CallRequest {
                        from: Some(key.public_address()),
                        to: options.to,
                        gas: None,
                        gas_price: gas_price.value(),
                        value: options.value,
                        data: options.data.clone(),
                    },
                    None,
                )
                .await?
        }
    };
    let gas_price = gas_price.resolve(&web3).await?;

    let signed = web3
        .accounts()
        .sign_transaction(
            TransactionParameters {
                nonce: options.nonce,
                gas_price: Some(gas_price),
                gas,
                to: options.to,
                value: options.value.unwrap_or_default(),
                data: options.data.unwrap_or_default(),
                chain_id,
            },
            SecretKeyRef::new(&key),
        )
        .await?;

    Ok(signed.raw_transaction)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::prelude::*;

    #[test]
    fn tx_build_local() {
        let transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let from = addr!("0x9876543210987654321098765432109876543210");

        let tx = build_transaction_request_for_local_signing(
            web3,
            Some(from),
            GasPrice::Standard,
            TransactionRequestOptions::default(),
        )
        .immediate()
        .expect("failed to build local transaction");

        transport.assert_no_more_requests();
        assert_eq!(tx.from, from);
    }

    #[test]
    fn tx_build_local_default_account() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let accounts = [
            addr!("0x9876543210987654321098765432109876543210"),
            addr!("0x1111111111111111111111111111111111111111"),
            addr!("0x2222222222222222222222222222222222222222"),
        ];

        transport.add_response(json!(accounts)); // get accounts
        let tx = build_transaction_request_for_local_signing(
            web3,
            None,
            GasPrice::Standard,
            TransactionRequestOptions::default(),
        )
        .immediate()
        .expect("failed to build local transaction");

        transport.assert_request("eth_accounts", &[]);
        transport.assert_no_more_requests();

        assert_eq!(tx.from, accounts[0]);
        assert_eq!(tx.gas_price, None);
    }

    #[test]
    fn tx_build_local_default_account_with_extra_gas_price() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let accounts = [
            addr!("0x9876543210987654321098765432109876543210"),
            addr!("0x1111111111111111111111111111111111111111"),
            addr!("0x2222222222222222222222222222222222222222"),
        ];

        transport.add_response(json!(accounts)); // get accounts
        transport.add_response(json!("0x42")); // gas price
        let tx = build_transaction_request_for_local_signing(
            web3,
            None,
            GasPrice::Scaled(2.0),
            TransactionRequestOptions::default(),
        )
        .immediate()
        .expect("failed to build local transaction");

        transport.assert_request("eth_accounts", &[]);
        transport.assert_request("eth_gasPrice", &[]);
        transport.assert_no_more_requests();

        assert_eq!(tx.from, accounts[0]);
        assert_eq!(tx.gas_price, Some(U256::from(0x42 * 2)));
    }

    #[test]
    fn tx_build_local_with_extra_gas_price() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let from = addr!("0xffffffffffffffffffffffffffffffffffffffff");

        transport.add_response(json!("0x42")); // gas price
        let tx = build_transaction_request_for_local_signing(
            web3,
            Some(from),
            GasPrice::Scaled(2.0),
            TransactionRequestOptions::default(),
        )
        .immediate()
        .expect("failed to build local transaction");

        transport.assert_request("eth_gasPrice", &[]);
        transport.assert_no_more_requests();

        assert_eq!(tx.from, from);
        assert_eq!(tx.gas_price, Some(U256::from(0x42 * 2)));
    }

    #[test]
    fn tx_build_local_with_explicit_gas_price() {
        let transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let from = addr!("0xffffffffffffffffffffffffffffffffffffffff");

        let tx = build_transaction_request_for_local_signing(
            web3,
            Some(from),
            GasPrice::Value(1337.into()),
            TransactionRequestOptions::default(),
        )
        .immediate()
        .expect("failed to build local transaction");

        transport.assert_no_more_requests();

        assert_eq!(tx.from, from);
        assert_eq!(tx.gas_price, Some(1337.into()));
    }

    #[test]
    fn tx_build_local_no_local_accounts() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        transport.add_response(json!([])); // get accounts
        let err = build_transaction_request_for_local_signing(
            web3,
            None,
            GasPrice::Standard,
            TransactionRequestOptions::default(),
        )
        .immediate()
        .expect_err("unexpected success building transaction");

        transport.assert_request("eth_accounts", &[]);
        transport.assert_no_more_requests();

        assert!(
            match err {
                ExecutionError::NoLocalAccounts => true,
                _ => false,
            },
            "expected no local accounts error but got '{:?}'",
            err
        );
    }

    #[test]
    fn tx_build_locked() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let from = addr!("0x9876543210987654321098765432109876543210");
        let pw = "foobar";
        let to = addr!("0x0000000000000000000000000000000000000000");
        let signed = bytes!("0x0123456789"); // doesn't have to be valid, we don't check

        transport.add_response(json!({
            "raw": signed,
            "tx": {
                "hash": "0x0000000000000000000000000000000000000000000000000000000000000000",
                "nonce": "0x0",
                "from": from,
                "value": "0x0",
                "gas": "0x0",
                "gasPrice": "0x0",
                "input": "0x",
            }
        })); // sign transaction
        let tx = build_transaction_signed_with_locked_account(
            web3,
            from,
            pw.into(),
            GasPrice::Standard,
            TransactionRequestOptions(
                TransactionOptions {
                    to: Some(to),
                    ..Default::default()
                },
                None,
            ),
        )
        .immediate()
        .expect("failed to build locked transaction");

        transport.assert_request(
            "personal_signTransaction",
            &[
                json!({
                    "from": from,
                    "to": to,
                }),
                json!(pw),
            ],
        );
        transport.assert_no_more_requests();

        assert_eq!(tx, signed);
    }

    #[test]
    fn tx_build_locked_with_extra_gas_price() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let from = addr!("0x9876543210987654321098765432109876543210");
        let pw = "foobar";
        let gas_price = U256::from(1337);
        let signed = bytes!("0x0123456789"); // doesn't have to be valid, we don't check

        transport.add_response(json!(gas_price));
        transport.add_response(json!({
            "raw": signed,
            "tx": {
                "hash": "0x0000000000000000000000000000000000000000000000000000000000000000",
                "nonce": "0x0",
                "from": from,
                "value": "0x0",
                "gas": "0x0",
                "gasPrice": gas_price,
                "input": "0x",
            }
        })); // sign transaction
        let tx = build_transaction_signed_with_locked_account(
            web3,
            from,
            pw.into(),
            GasPrice::Scaled(2.0),
            TransactionRequestOptions::default(),
        )
        .immediate()
        .expect("failed to build locked transaction");

        transport.assert_request("eth_gasPrice", &[]);
        transport.assert_request(
            "personal_signTransaction",
            &[
                json!({
                    "from": from,
                    "gasPrice": gas_price * 2,
                }),
                json!(pw),
            ],
        );
        transport.assert_no_more_requests();

        assert_eq!(tx, signed);
    }

    #[test]
    fn tx_build_offline() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let key = key!("0x0102030405060708091011121314151617181920212223242526272829303132");
        let from: Address = key.public_address();
        let to = addr!("0x0000000000000000000000000000000000000000");

        let gas = uint!("0x9a5");
        let gas_price = uint!("0x1ce");
        let nonce = uint!("0x42");
        let chain_id = 77777;

        transport.add_response(json!(gas));
        transport.add_response(json!(gas_price * 2));
        transport.add_response(json!(nonce));
        transport.add_response(json!(format!("{:#x}", chain_id)));

        let tx1 = build_offline_signed_transaction(
            web3.clone(),
            key.clone(),
            None,
            GasPrice::Standard,
            TransactionOptions {
                to: Some(to),
                ..Default::default()
            },
        )
        .immediate()
        .expect("failed to build offline transaction");

        // assert that we ask the node for all the missing values
        transport.assert_request(
            "eth_estimateGas",
            &[json!({
                "from": from,
                "to": to,
            })],
        );
        transport.assert_request("eth_gasPrice", &[]);
        transport.assert_request("eth_getTransactionCount", &[json!(from), json!("latest")]);
        transport.assert_request("eth_chainId", &[]);
        transport.assert_no_more_requests();

        transport.add_response(json!(gas_price));

        let tx2 = build_offline_signed_transaction(
            web3.clone(),
            key.clone(),
            Some(chain_id),
            GasPrice::Scaled(2.0),
            TransactionOptions {
                to: Some(to),
                gas: Some(gas),
                nonce: Some(nonce),
                ..Default::default()
            },
        )
        .immediate()
        .expect("failed to build offline transaction");

        transport.assert_request("eth_gasPrice", &[]);
        transport.assert_no_more_requests();

        let tx3 = build_offline_signed_transaction(
            web3,
            key,
            Some(chain_id),
            GasPrice::Value(gas_price * 2),
            TransactionOptions {
                to: Some(to),
                gas: Some(gas),
                nonce: Some(nonce),
                ..Default::default()
            },
        )
        .immediate()
        .expect("failed to build offline transaction");

        // assert that if we provide all the values then we can sign right away
        transport.assert_no_more_requests();

        // check that if we sign with same values we get same results
        assert_eq!(tx1, tx2);
        assert_eq!(tx2, tx3);
    }
}
