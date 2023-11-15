//! This module implements transaction finalization from partial transaction
//! parameters. It provides futures for building `TransactionRequest` instances
//! and raw `Bytes` transactions from partial transaction parameters, where the
//! remaining parameters are queried from the node before finalizing the
//! transaction.

use crate::errors::ExecutionError;
use crate::secret::{Password, PrivateKey};
use crate::transaction::gas_price::GasPrice;
#[cfg(feature = "aws-kms")]
use crate::transaction::kms;
use crate::transaction::{Account, TransactionBuilder};
use web3::api::Web3;
use web3::types::{
    AccessList, Address, Bytes, CallRequest, RawTransaction, SignedTransaction,
    TransactionCondition, TransactionParameters, TransactionRequest, H256, U256,
};
use web3::Transport;

impl<T: Transport> TransactionBuilder<T> {
    /// Build a prepared transaction that is ready to send.
    ///
    /// Can resolve into either a `TransactionRequest` for sending locally
    /// signed transactions or raw signed transaction `Bytes` when sending a raw
    /// transaction.
    pub async fn build(self) -> Result<Transaction, ExecutionError> {
        let options = TransactionOptions {
            to: self.to,
            gas: self.gas,
            gas_price: self.gas_price,
            value: self.value,
            data: self.data,
            nonce: self.nonce,
            access_list: self.access_list,
        };

        let tx = match self.from {
            None => Transaction::Request(
                build_transaction_request_for_local_signing(
                    self.web3,
                    None,
                    TransactionRequestOptions(options, None),
                )
                .await?,
            ),
            Some(Account::Local(from, condition)) => Transaction::Request(
                build_transaction_request_for_local_signing(
                    self.web3,
                    Some(from),
                    TransactionRequestOptions(options, condition),
                )
                .await?,
            ),
            Some(Account::Locked(from, password, condition)) => {
                build_transaction_signed_with_locked_account(
                    self.web3,
                    from,
                    password,
                    TransactionRequestOptions(options, condition),
                )
                .await
                .map(|signed| Transaction::Raw {
                    bytes: signed.raw,
                    hash: signed.tx.hash,
                })?
            }
            Some(Account::Offline(key, chain_id)) => {
                build_offline_signed_transaction(self.web3, key, chain_id, options)
                    .await
                    .map(|signed| Transaction::Raw {
                        bytes: signed.raw_transaction,
                        hash: signed.transaction_hash,
                    })?
            }
            #[cfg(feature = "aws-kms")]
            Some(Account::Kms(account, chain_id)) => {
                build_kms_signed_transaction(self.web3, account, chain_id, options)
                    .await
                    .map(|signed| Transaction::Raw {
                        bytes: signed.raw_transaction,
                        hash: signed.transaction_hash,
                    })?
            }
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
    Raw {
        /// The raw signed transaction bytes
        bytes: Bytes,
        /// The transaction hash
        hash: H256,
    },
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
            Transaction::Raw { bytes, .. } => Some(bytes),
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
    /// Optional gas price to use for transaction.
    pub gas_price: Option<GasPrice>,
    /// The ETH value to send with the transaction.
    pub value: Option<U256>,
    /// The data for the transaction.
    pub data: Option<Bytes>,
    /// The transaction nonce.
    pub nonce: Option<U256>,
    /// The access list
    pub access_list: Option<AccessList>,
}

/// Transaction options specific to `TransactionRequests` since they may also
/// include a `TransactionCondition` that is not applicable to raw signed
/// transactions.
#[derive(Clone, Debug, Default)]
struct TransactionRequestOptions(TransactionOptions, Option<TransactionCondition>);

impl TransactionRequestOptions {
    /// Builds a `TransactionRequest` from a `TransactionRequestOptions` by
    /// specifying the missing parameters.
    fn build_request(self, from: Address, gas: Option<U256>) -> TransactionRequest {
        let resolved_gas_price = self
            .0
            .gas_price
            .map(|gas_price| gas_price.resolve_for_transaction())
            .unwrap_or_default();
        TransactionRequest {
            from,
            to: self.0.to,
            gas,
            gas_price: resolved_gas_price.gas_price,
            value: self.0.value,
            data: self.0.data,
            nonce: self.0.nonce,
            condition: self.1,
            transaction_type: resolved_gas_price.transaction_type,
            access_list: self.0.access_list,
            max_fee_per_gas: resolved_gas_price.max_fee_per_gas,
            max_priority_fee_per_gas: resolved_gas_price.max_priority_fee_per_gas,
        }
    }
}

/// Build a transaction request to locally signed by the node before sending.
async fn build_transaction_request_for_local_signing<T: Transport>(
    web3: Web3<T>,
    from: Option<Address>,
    options: TransactionRequestOptions,
) -> Result<TransactionRequest, ExecutionError> {
    let from = match from {
        Some(address) => address,
        None => *web3
            .eth()
            .accounts()
            .await?
            .first()
            .ok_or(ExecutionError::NoLocalAccounts)?,
    };
    let gas = resolve_gas_limit(&web3, from, &options.0).await?;
    let request = options.build_request(from, Some(gas));

    Ok(request)
}

/// Build a locally signed transaction with a locked account.
async fn build_transaction_signed_with_locked_account<T: Transport>(
    web3: Web3<T>,
    from: Address,
    password: Password,
    options: TransactionRequestOptions,
) -> Result<RawTransaction, ExecutionError> {
    let gas = resolve_gas_limit(&web3, from, &options.0).await?;
    let request = options.build_request(from, Some(gas));
    let signed_tx = web3.personal().sign_transaction(request, &password).await?;

    Ok(signed_tx)
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
    options: TransactionOptions,
) -> Result<SignedTransaction, ExecutionError> {
    let gas = resolve_gas_limit(&web3, key.public_address(), &options).await?;
    let resolved_gas_price = options
        .gas_price
        .map(|gas_price| gas_price.resolve_for_transaction())
        .unwrap_or_default();
    let signed = web3
        .accounts()
        .sign_transaction(
            TransactionParameters {
                nonce: options.nonce,
                gas_price: resolved_gas_price.gas_price,
                gas,
                to: options.to,
                value: options.value.unwrap_or_default(),
                data: options.data.unwrap_or_default(),
                chain_id,
                transaction_type: resolved_gas_price.transaction_type,
                access_list: options.access_list,
                max_fee_per_gas: resolved_gas_price.max_fee_per_gas,
                max_priority_fee_per_gas: resolved_gas_price.max_priority_fee_per_gas,
            },
            &key,
        )
        .await?;

    Ok(signed)
}

/// Build a KMS signed transaction.
///
/// Note that all transaction parameters must be finalized before signing. This
/// means that things like account nonce, gas and gas price estimates, as well
/// as chain ID must be queried from the node if not provided before signing.
#[cfg(feature = "aws-kms")]
async fn build_kms_signed_transaction<T: Transport>(
    web3: Web3<T>,
    account: kms::Account,
    chain_id: Option<u64>,
    options: TransactionOptions,
) -> Result<SignedTransaction, ExecutionError> {
    let gas = resolve_gas_limit(&web3, account.public_address(), &options).await?;
    let resolved_gas_price = options
        .gas_price
        .map(|gas_price| gas_price.resolve_for_transaction())
        .unwrap_or_default();
    let signed = account
        .sign_transaction(
            web3,
            TransactionParameters {
                nonce: options.nonce,
                gas_price: resolved_gas_price.gas_price,
                gas,
                to: options.to,
                value: options.value.unwrap_or_default(),
                data: options.data.unwrap_or_default(),
                chain_id,
                transaction_type: resolved_gas_price.transaction_type,
                access_list: options.access_list,
                max_fee_per_gas: resolved_gas_price.max_fee_per_gas,
                max_priority_fee_per_gas: resolved_gas_price.max_priority_fee_per_gas,
            },
        )
        .await?;

    Ok(signed)
}

async fn resolve_gas_limit<T: Transport>(
    web3: &Web3<T>,
    from: Address,
    options: &TransactionOptions,
) -> Result<U256, ExecutionError> {
    let resolved_gas_price = options
        .gas_price
        .map(|gas_price| gas_price.resolve_for_transaction())
        .unwrap_or_default();
    match options.gas {
        Some(value) => Ok(value),
        None => Ok(web3
            .eth()
            .estimate_gas(
                CallRequest {
                    from: Some(from),
                    to: options.to,
                    gas: None,
                    gas_price: resolved_gas_price.gas_price,
                    value: options.value,
                    data: options.data.clone(),
                    transaction_type: resolved_gas_price.transaction_type,
                    access_list: options.access_list.clone(),
                    max_fee_per_gas: resolved_gas_price.max_fee_per_gas,
                    max_priority_fee_per_gas: resolved_gas_price.max_priority_fee_per_gas,
                },
                None,
            )
            .await?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::prelude::*;

    #[test]
    fn tx_build_local() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let from = addr!("0x9876543210987654321098765432109876543210");

        transport.add_response(json!("0x9a5")); // gas limit

        let tx = build_transaction_request_for_local_signing(
            web3,
            Some(from),
            TransactionRequestOptions::default(),
        )
        .immediate()
        .expect("failed to build local transaction");

        transport.assert_request(
            "eth_estimateGas",
            &[json!({"from": "0x9876543210987654321098765432109876543210"})],
        );
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
        transport.add_response(json!("0x9a5")); // gas limit
        let tx = build_transaction_request_for_local_signing(
            web3,
            None,
            TransactionRequestOptions::default(),
        )
        .immediate()
        .expect("failed to build local transaction");

        transport.assert_request("eth_accounts", &[]);
        transport.assert_request(
            "eth_estimateGas",
            &[json!({"from": "0x9876543210987654321098765432109876543210"})],
        );
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
        transport.add_response(json!("0x9a5")); // gas limit
        let tx = build_transaction_request_for_local_signing(
            web3,
            None,
            TransactionRequestOptions {
                0: TransactionOptions {
                    gas_price: Some(66.0.into()),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .immediate()
        .expect("failed to build local transaction");

        transport.assert_request("eth_accounts", &[]);
        transport.assert_request(
            "eth_estimateGas",
            &[json!({ "from": json!(accounts[0]), "gasPrice": format!("{:#x}", 66), })],
        );
        transport.assert_no_more_requests();

        assert_eq!(tx.from, accounts[0]);
        assert_eq!(tx.gas_price, Some(U256::from(0x42)));
        assert_eq!(tx.gas, Some(U256::from(0x9a5)));
    }

    #[test]
    fn tx_build_local_with_explicit_gas_price() {
        let mut transport = TestTransport::new();
        let web3 = Web3::new(transport.clone());

        let from = addr!("0xffffffffffffffffffffffffffffffffffffffff");

        transport.add_response(json!("0x9a5")); // gas limit

        let options = TransactionRequestOptions {
            0: TransactionOptions {
                gas_price: Some(1337.0.into()),
                ..Default::default()
            },
            ..Default::default()
        };

        let tx = build_transaction_request_for_local_signing(web3, Some(from), options)
            .immediate()
            .expect("failed to build local transaction");

        transport.assert_request(
            "eth_estimateGas",
            &[json!({ "from": json!(from) , "gasPrice": format!("{:#x}", 1337)})],
        );
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
            TransactionRequestOptions::default(),
        )
        .immediate()
        .expect_err("unexpected success building transaction");

        transport.assert_request("eth_accounts", &[]);
        transport.assert_no_more_requests();

        assert!(
            matches!(err, ExecutionError::NoLocalAccounts),
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
        let hash = H256::from_low_u64_be(1);
        let gas = json!("0x9a5");

        transport.add_response(gas.clone());
        transport.add_response(json!({
            "raw": signed,
            "tx": {
                "hash": "0x0000000000000000000000000000000000000000000000000000000000000001",
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
            "eth_estimateGas",
            &[json!({ "from": json!(from) , "to": json!(to)})],
        );
        transport.assert_request(
            "personal_signTransaction",
            &[
                json!({
                    "from": from,
                    "to": to,
                    "gas": gas
                }),
                json!(pw),
            ],
        );
        transport.assert_no_more_requests();

        assert_eq!(tx.raw, signed);
        assert_eq!(tx.tx.hash, hash);
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
        transport.add_response(json!(nonce));
        transport.add_response(json!(format!("{:#x}", chain_id)));

        let tx1 = build_offline_signed_transaction(
            web3.clone(),
            key.clone(),
            None,
            TransactionOptions {
                to: Some(to),
                gas_price: Some(gas_price.into()),
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
                "gasPrice": gas_price,
            })],
        );
        transport.assert_request("eth_getTransactionCount", &[json!(from), json!("latest")]);
        transport.assert_request("eth_chainId", &[]);
        transport.assert_no_more_requests();

        let tx2 = build_offline_signed_transaction(
            web3,
            key,
            Some(chain_id),
            TransactionOptions {
                to: Some(to),
                gas: Some(gas),
                gas_price: Some(gas_price.into()),
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
    }
}
