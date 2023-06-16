//! AWS KMS account implementation.
//!
//! The implementation was heavily guided by this guide for doing the same in javascript:
//! https://luhenning.medium.com/the-dark-side-of-the-elliptic-curve-signing-ethereum-transactions-with-aws-kms-in-javascript-83610d9a6f81
//!
//! It's quite hacky, however the hackiness does not leak outside this module.

use asn1::{OwnedBitString, Tlv};
use aws_sdk_kms::{
    primitives::Blob,
    types::{KeySpec, KeyUsageType, MessageType, SigningAlgorithmSpec},
    Client, Config,
};
use ethcontract_common::hash::keccak256;
use primitive_types::U256;
use rlp::{Rlp, RlpStream};
use secp256k1::constants::CURVE_ORDER;
use web3::{
    signing::{self, Signature},
    types::{Address, Bytes, SignedTransaction, TransactionParameters, H256},
    Transport, Web3,
};

use crate::errors::ExecutionError;

/// An AWS KMS account abstraction.
#[derive(Clone, Debug)]
pub struct Account {
    client: Client,
    key_id: String,
    address: Address,
}

#[allow(dead_code)]
#[derive(asn1::Asn1Read)]
struct AlgorithmIdentifier<'a> {
    algorithm: asn1::ObjectIdentifier,
    parameters: Tlv<'a>,
}

#[allow(dead_code)]
#[derive(asn1::Asn1Read)]
struct DerEncodedPublicKey<'a> {
    algorithm: AlgorithmIdentifier<'a>,
    subject_public_key: OwnedBitString,
}

impl Account {
    /// Creates a new KMS account.
    pub async fn new(config: Config, key_id: &str) -> Result<Self, Error> {
        let client = Client::from_conf(config);
        let key_id = key_id.to_string();
        let key = client
            .get_public_key()
            .key_id(&key_id)
            .send()
            .await
            .map_err(aws_sdk_kms::Error::from)?;

        if !matches!(
            (key.key_spec(), key.key_usage()),
            (
                Some(&KeySpec::EccSecgP256K1),
                Some(&KeyUsageType::SignVerify),
            ),
        ) {
            return Err(Error::InvalidKey);
        }

        // The private key block is an DER-encoded X.509 public key (also known as `SubjectPublicKeyInfo`, as defined in RFC 5280).
        // Its first byte indicates that this is an uncompressed key. We need to remove this byte for the public key to be correct.
        // Once we delete the first byte, we get the raw public key that can be used to calculate our Ethereum address, which is equal to
        // the last 20 bytes of the keccak256 hash of the raw public key.
        let result = asn1::parse_single::<DerEncodedPublicKey>(key.public_key().unwrap().as_ref())
            .or(Err(Error::InvalidKey))?;
        let uncompressed = Vec::from(result.subject_public_key.as_bitstring().as_bytes());
        let address = {
            let mut buffer = Address::default();
            let hash = keccak256(&uncompressed[1..]);
            buffer.0.copy_from_slice(&hash[12..]);
            buffer
        };

        Ok(Self {
            client,
            key_id,
            address,
        })
    }

    /// Returns the public address of the AWS KMS account.
    pub fn public_address(&self) -> Address {
        self.address
    }

    /// Signs a message.
    pub async fn sign(&self, message: [u8; 32]) -> Result<Signature, Error> {
        let output = self
            .client
            .sign()
            .key_id(&self.key_id)
            .message(Blob::new(message))
            .message_type(MessageType::Digest)
            .signing_algorithm(SigningAlgorithmSpec::EcdsaSha256)
            .send()
            .await
            .map_err(aws_sdk_kms::Error::from)
            .unwrap();
        let signature = secp256k1::ecdsa::Signature::from_der(
            output.signature().ok_or(Error::InvalidSignature)?.as_ref(),
        )
        .map_err(|_| Error::InvalidSignature)?;

        let compact = signature.serialize_compact();
        let mut r = H256::default();
        r.0.copy_from_slice(&compact[..32]);

        // If `s` happens to be "on the dark side of the curve", we need to invert it (EIP-2)
        let mut s_tentative: U256 = U256::from(&compact[32..]);
        let secp256k1_n = U256::from(CURVE_ORDER);
        if s_tentative > secp256k1_n / 2 {
            s_tentative = secp256k1_n - s_tentative;
        }
        let mut s = H256::default();
        s_tentative.to_big_endian(&mut s.0);

        // Recover correct v by trying which of the two options recovers to our public key
        let v = if signing::recover(&message, &[r.as_bytes(), s.as_bytes()].concat(), 0).ok()
            == Some(self.address)
        {
            0
        } else if signing::recover(&message, &[r.as_bytes(), s.as_bytes()].concat(), 1).ok()
            == Some(self.address)
        {
            1
        } else {
            return Err(Error::InvalidSignature);
        };

        Ok(Signature { v, r, s })
    }

    /// Signs a transaction.
    pub async fn sign_transaction<T>(
        &self,
        web3: Web3<T>,
        params: TransactionParameters,
    ) -> Result<SignedTransaction, Error>
    where
        T: Transport,
    {
        // Note that we build a signed transaction with a dummy signature. We
        // make use of the returned raw transaction and signing message to
        // actually generate a signature using AWS KMS.
        let transaction = web3.accounts().sign_transaction(params, Key(self)).await?;
        let signature = self.sign(transaction.message_hash.0).await?;

        // transaction.v has the EIP155 value w/ 0 parity, signature.v has the parity bit, together they from the correct v
        let v = transaction.v + signature.v;

        // Split the transaction into its ID and its raw RLP encoded form.
        let (id, raw) = match transaction.raw_transaction.0.first().copied() {
            Some(x) if x < 0x80 => (Some(x), &transaction.raw_transaction.0[1..]),
            _ => (None, &transaction.raw_transaction.0[..]),
        };

        // Fortunately for us, raw transactions always RLP append the signature,
        // meaning the last 3 list values are `v`, `r`, and `s` respectively.
        // Re-encode the transaction, replacing the last 3 list values.
        let len = match Rlp::new(raw).prototype()? {
            rlp::Prototype::List(len) => len
                .checked_sub(3)
                .ok_or(rlp::DecoderError::Custom("transaction fields too short"))?,
            _ => return Err(rlp::DecoderError::RlpExpectedToBeList.into()),
        };
        let mut encoder = RlpStream::new_list(len + 3);
        for item in Rlp::new(raw).iter().take(len) {
            encoder.append_raw(item.as_raw(), 1);
        }
        encoder.append(&v);
        encoder.append(&signature.r);
        encoder.append(&signature.s);

        let raw_transaction = Bytes(match id {
            Some(id) => [&[id], encoder.as_raw()].concat(),
            None => encoder.out().to_vec(),
        });
        let transaction_hash = H256(keccak256(&raw_transaction.0));

        Ok(SignedTransaction {
            message_hash: transaction.message_hash,
            v,
            r: signature.r,
            s: signature.s,
            raw_transaction,
            transaction_hash,
        })
    }
}

/// A web3 signing key adapter.
///
/// The `web3` crate has utility methods for building and RLP encoding signed
/// transactions that we want to reuse. Unfortunately it expects `sign`-ing to
/// **not** be asynchronous. To work around this, we create this adapter that
/// returns dummy signatures including the all-important signing message and
/// then use AWS KMS to sign the transaction and adjust the returned
/// `SignedTransaction` result.
///
/// Ugly, but effective...
struct Key<'a>(&'a Account);

impl web3::signing::Key for Key<'_> {
    fn sign(
        &self,
        message: &[u8],
        chain_id: Option<u64>,
    ) -> Result<Signature, web3::signing::SigningError> {
        let signature = self.sign_message(message)?;
        // apply EIP155
        Ok(Signature {
            v: if let Some(chain_id) = chain_id {
                signature.v + 35 + chain_id * 2
            } else {
                signature.v + 27
            },
            ..signature
        })
    }

    fn sign_message(&self, _: &[u8]) -> Result<Signature, web3::signing::SigningError> {
        Ok(Signature {
            r: H256::default(),
            s: H256::default(),
            v: 0,
        })
    }

    fn address(&self) -> Address {
        self.0.public_address()
    }
}

/// Error type for when KMS Signing fails
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Error related to the KMS interactions
    #[error(transparent)]
    Kms(#[from] aws_sdk_kms::Error),
    /// Error related to the Key stored in KMS being incorrect
    #[error("invalid key")]
    InvalidKey,
    /// Error related to the signing algorithm used
    #[error("invalid signature")]
    InvalidSignature,
    /// Error related to the Web3 interactions needed for signing
    #[error(transparent)]
    Web3(#[from] web3::error::Error),
    /// Error related to decoding the transaction object
    #[error(transparent)]
    Rlp(#[from] rlp::DecoderError),
}

impl From<Error> for ExecutionError {
    fn from(_: Error) -> Self {
        web3::error::Error::Internal.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction::{self, TransactionBuilder};
    use std::env;
    use web3::transports;

    #[tokio::test]
    async fn example() {
        let config = aws_config::load_from_env().await;
        let account = Account::new((&config).into(), &env::var("KMS_KEY_ID").unwrap())
            .await
            .unwrap();

        println!("{:?}", account.public_address());

        let web3 = {
            let url = env::var("NODE_URL").unwrap();
            let http = transports::Http::new(&url).expect("transport failed");
            Web3::new(http)
        };
        let chain_id = web3.eth().chain_id().await.expect("Failed to get chainID");
        TransactionBuilder::new(web3)
            .from(transaction::Account::Kms(
                account.clone(),
                Some(chain_id.as_u64()),
            ))
            .to(account.public_address())
            .send()
            .await
            .unwrap();

        panic!("{:?}", account.public_address());
    }
}
