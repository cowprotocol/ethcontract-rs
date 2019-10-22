//! Utility for signing transactions and generating RLP encoded raw transactions.
//! Hopefully we can move this functionailly upstream to the `web3` crate as
//! part of the missing `accounts` namespace.

use ethsign::{Error as EthsignError, SecretKey, Signature};
use rlp::RlpStream;
use web3::types::{Address, Bytes, U256};

/// Raw transaction data to sign
pub struct TransactionData<'a> {
    /// Nonce to use when signing this transaction.
    pub nonce: U256,
    /// Gas price to use when signing this transaction.
    pub gas_price: U256,
    /// Gas provided by the transaction.
    pub gas: U256,
    /// Receiver of the transaction.
    pub to: Address,
    /// Value of the transaction in wei.
    pub value: U256,
    /// Call data of the transaction, can be empty for simple value transfers.
    pub data: &'a Bytes,
}

impl<'a> TransactionData<'a> {
    /// Sign and return a raw transaction.
    pub fn sign(&self, key: &SecretKey, chain_id: Option<u64>) -> Result<Bytes, EthsignError> {
        let mut rlp = RlpStream::new();
        self.rlp_append_unsigned(&mut rlp, chain_id);
        let hash = tiny_keccak::keccak256(&rlp.as_raw());
        rlp.clear();

        let sig = key.sign(&hash[..])?;
        self.rlp_append_signed(&mut rlp, sig, chain_id);

        Ok(rlp.out().into())
    }

    /// RLP encode an unsigned transaction.
    fn rlp_append_unsigned(&self, s: &mut RlpStream, chain_id: Option<u64>) {
        s.begin_list(if chain_id.is_some() { 9 } else { 6 });
        s.append(&self.nonce);
        s.append(&self.gas_price);
        s.append(&self.gas);
        s.append(&self.to);
        s.append(&self.value);
        s.append(&self.data.0);
        if let Some(n) = chain_id {
            s.append(&n);
            s.append(&0u8);
            s.append(&0u8);
        }
    }

    /// RLP encode a transaction with its signature.
    fn rlp_append_signed(&self, s: &mut RlpStream, sig: Signature, chain_id: Option<u64>) {
        let v = add_chain_replay_protection(sig.v as _, chain_id);

        s.begin_list(9);
        s.append(&self.nonce);
        s.append(&self.gas_price);
        s.append(&self.gas);
        s.append(&self.to);
        s.append(&self.value);
        s.append(&self.data.0);
        s.append(&v);
        s.append(&U256::from(sig.r));
        s.append(&U256::from(sig.s));
    }
}

/// Encode chain ID based on (EIP-155)[https://github.com/ethereum/EIPs/blob/master/EIPS/eip-155.md)
fn add_chain_replay_protection(v: u64, chain_id: Option<u64>) -> u64 {
    v + if let Some(n) = chain_id {
        35 + n * 2
    } else {
        27
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use web3::types::H256;

    #[test]
    fn test_sign() {
        // retrieved test vector from:
        // https://web3js.readthedocs.io/en/v1.2.0/web3-eth-accounts.html#eth-accounts-signtransaction

        let tx = TransactionData {
            nonce: 0.into(),
            gas: 2_000_000.into(),
            gas_price: 234_567_897_654_321u64.into(),
            to: "F0109fC8DF283027b6285cc889F5aA624EaC1F55"
                .parse()
                .expect("valid address"),
            value: 1_000_000_000.into(),
            data: &Bytes::default(),
        };
        let key: H256 = "4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318"
            .parse()
            .expect("valid bytes");
        let raw = tx
            .sign(&SecretKey::from_raw(&key[..]).expect("valid key"), Some(1))
            .expect("can sign");

        let expected: Bytes = serde_json::from_str(r#""0xf86a8086d55698372431831e848094f0109fc8df283027b6285cc889f5aa624eac1f55843b9aca008025a009ebb6ca057a0535d6186462bc0b465b561c94a295bdb0621fc19208ab149a9ca0440ffd775ce91a833ab410777204d5341a6f9fa91216a6f3ee2c051fea6a0428""#).expect("valid raw transaction");

        assert_eq!(raw, expected);
    }
}
