//! AWS KMS account implementation.
//!
//! This implementation is very hacky... However, the hackiness does not leak
//! outside this module, so it is OK :).

use aws_sdk_kms::Client;
use rlp::{Rlp, RlpStream};
use web3::{
    signing::Signature,
    types::{Address, Bytes, SignedTransaction, TransactionParameters, H256},
    Transport, Web3,
};

/// An AWS KMS account abstraction.
#[derive(Clone, Debug)]
pub struct Account {
    client: Client,
    address: Address,
}

impl Account {
    /// Returns the public address of the AWS KMS account.
    pub fn public_address(&self) -> Address {
        self.address
    }

    /// Signs a message.
    pub async fn sign(&self, message: [u8; 32]) -> Result<Signature, Error> {
        todo!()
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
        let v = transaction.v + signature.v;

        // Split the transaction into its ID and its raw RLP encoded form.
        let (id, raw) = match transaction.raw_transaction.0.get(0).copied() {
            Some(x) if x < 0x80 => (Some(x), &transaction.raw_transaction.0[1..]),
            _ => (None, &transaction.raw_transaction.0[..]),
        };

        // Fortunately for us, raw transactions always RLP append the signature,
        // meaning the last 3 list values are `v`, `r`, and `s` respectively.
        // Re-encode the transaction, replacing the last 3 list values.
        let len = match Rlp::new(&raw).prototype()? {
            rlp::Prototype::List(len) => len
                .checked_sub(3)
                .ok_or_else(|| rlp::DecoderError::Custom("transaction fields too short"))?,
            _ => return Err(rlp::DecoderError::RlpExpectedToBeList.into()),
        };
        let mut encoder = RlpStream::new_list(len + 3);
        for item in Rlp::new(&raw).iter().take(len) {
            encoder.append_raw(item.as_raw(), 1);
        }
        encoder.append(&v);
        encoder.append(&signature.r);
        encoder.append(&signature.s);

        let raw_transaction = Bytes(match id {
            Some(id) => [&[id], encoder.as_raw()].concat(),
            None => encoder.out().to_vec(),
        });
        let transaction_hash = H256(ethcontract_common::hash::keccak256(&raw_transaction.0));

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

/// Replace an occurrence of a 32-byte sub-slice with another. This method
/// searches from the end of the slice and returns the position where the
/// replacing happened.
fn rreplace(slice: &mut [u8], from: &[u8; 32], to: &[u8; 32]) -> Option<usize> {
    let (start, _) = slice
        .windows(32)
        .enumerate()
        .rev()
        .find(|(_, s)| s == from)?;
    slice[start..][..32].copy_from_slice(to);
    Some(start)
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Web3(#[from] web3::error::Error),
    #[error(transparent)]
    Rlp(#[from] rlp::DecoderError),
}
