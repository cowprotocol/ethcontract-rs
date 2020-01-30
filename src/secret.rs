//! This module implements secrets in the form of protected memory.

use crate::errors::InvalidPrivateKey;
use crate::hash;
use secp256k1::key::ONE_KEY;
use secp256k1::{PublicKey, Secp256k1, SecretKey};
use std::fmt::{self, Debug, Formatter};
use std::ops::Deref;
use std::str::FromStr;
use web3::types::Address;
use zeroize::{DefaultIsZeroes, Zeroizing};

/// A secret key used for signing and hashing.
///
/// This type has a safe `Debug` implementation that does not leak information.
/// Additionally, it implements `Drop` to zeroize the memory to make leaking
/// passwords less likely.
#[derive(Clone)]
pub struct PrivateKey(Zeroizing<ZeroizeSecretKey>);

impl PrivateKey {
    /// Creates a new private key from raw bytes.
    pub fn from_raw(raw: [u8; 32]) -> Result<Self, InvalidPrivateKey> {
        PrivateKey::from_slice(&raw)
    }

    /// Creates a new private key from a slice of bytes.
    pub fn from_slice<B: AsRef<[u8]>>(raw: B) -> Result<Self, InvalidPrivateKey> {
        let secret_key = SecretKey::from_slice(raw.as_ref())?;
        Ok(PrivateKey(Zeroizing::new(secret_key.into())))
    }

    /// Creates a new private key from a hex string representation. Accepts hex
    /// string with or without leading `"0x"`.
    pub fn from_hex_str<S: AsRef<str>>(s: S) -> Result<Self, InvalidPrivateKey> {
        let hex_str = {
            let s = s.as_ref();
            if s.starts_with("0x") {
                &s[2..]
            } else {
                s
            }
        };
        let secret_key = SecretKey::from_str(hex_str)?;
        Ok(PrivateKey(Zeroizing::new(secret_key.into())))
    }

    /// Gets the public address for a given private key.
    pub fn public_address(&self) -> Address {
        let secp = Secp256k1::signing_only();
        let public_key = PublicKey::from_secret_key(&secp, &*self).serialize_uncompressed();

        // NOTE: An ethereum address is the last 20 bytes of the keccak hash of
        //   the public key. Note that `libsecp256k1` public key is serialized
        //   into 65 bytes as the first byte is always 0x04 as a tag to mark a
        //   uncompressed public key. Discard it for the public address
        //   calculation.
        debug_assert_eq!(public_key[0], 0x04);
        let hash = hash::keccak256(&public_key[1..]);

        Address::from_slice(&hash[12..])
    }
}

impl FromStr for PrivateKey {
    type Err = InvalidPrivateKey;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        PrivateKey::from_hex_str(s)
    }
}

impl Deref for PrivateKey {
    type Target = SecretKey;

    fn deref(&self) -> &Self::Target {
        &(self.0).0
    }
}

impl Debug for PrivateKey {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_tuple("PrivateKey")
            .field(&self.public_address())
            .finish()
    }
}

/// An internal type that allows us to implement `Zeroize` on `SecretKey`. This
/// allows `PrivateKey` to correctly zeroize (almost, we use the `ONE_KEY`
/// instead of `0`s since it is the first valid key) in a way that does not get
/// optimized away by the compiler or get access reordered.
///
/// For more information, consult the `zeroize` crate
/// [`README`](https://github.com/iqlusioninc/crates/tree/develop/zeroize).
#[derive(Clone, Copy)]
struct ZeroizeSecretKey(SecretKey);

impl From<SecretKey> for ZeroizeSecretKey {
    fn from(secret_key: SecretKey) -> Self {
        ZeroizeSecretKey(secret_key)
    }
}

impl Default for ZeroizeSecretKey {
    fn default() -> Self {
        ONE_KEY.into()
    }
}

impl DefaultIsZeroes for ZeroizeSecretKey {}

/// A password string.
///
/// This type has a safe `Debug` implementation that does not leak information.
/// Additionally, it implements `Drop` to zeroize the memory to make leaking
/// passwords less likely.
#[derive(Clone)]
pub struct Password(Zeroizing<String>);

impl Password {
    /// Creates a new password from a string.
    pub fn new<S: Into<String>>(password: S) -> Self {
        Password(Zeroizing::new(password.into()))
    }
}

impl<T: Into<String>> From<T> for Password {
    fn from(value: T) -> Self {
        Password::new(value)
    }
}

impl AsRef<str> for Password {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Deref for Password {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Debug for Password {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_tuple("Password").field(&"********").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroize::Zeroize;

    #[test]
    fn private_key_address() {
        // retrieved test vector from both (since the two cited examples use the
        // same message and key - as the hashes and signatures match):
        // https://web3js.readthedocs.io/en/v1.2.5/web3-eth-accounts.html#sign
        // https://web3js.readthedocs.io/en/v1.2.5/web3-eth-accounts.html#recover
        let key = key!("0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318");
        let address = addr!("0x2c7536E3605D9C16a7a3D7b1898e529396a65c23");

        assert_eq!(key.public_address(), address);
    }

    #[test]
    fn drop_private_key() {
        let mut key = key!("0x0102030405060708091011121314151617181920212223242526272829303132");
        key.0.zeroize();
        assert_eq!(*key, ONE_KEY);
    }

    #[test]
    fn drop_password() {
        let mut pw = Password::new("foobar");
        pw.0.zeroize();
        assert_eq!(&*pw, "");
    }
}
