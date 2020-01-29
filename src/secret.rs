//! This module implements secrets in the form of protected memory.

use secp256k1::{Error as Secp256k1Error, SecretKey};
use std::fmt::{self, Debug, Formatter};
use std::ops::Deref;
use std::str::FromStr;
use web3::types::Address;
use zeroize::Zeroizing;

/// A secret key used for signing and hashing.
///
/// This type has a safe `Debug` implementation that does not leak information.
/// Additionally, it implements `Drop` to zeroize the memory to make leaking
/// passwords less likely.
#[derive(Clone)]
pub struct PrivateKey(SecretKey);

impl PrivateKey {
    /// Creates a new private key from raw bytes.
    pub fn from_raw(raw: [u8; 32]) -> Result<Self, Secp256k1Error> {
        PrivateKey::from_slice(&raw)
    }

    /// Creates a new private key from a slice of bytes.
    pub fn from_slice<B: AsRef<[u8]>>(raw: B) -> Result<Self, Secp256k1Error> {
        Ok(PrivateKey(SecretKey::from_slice(raw.as_ref())?))
    }

    /// Creates a new private key from a hex string representation. Accepts hex
    /// string with or without leading `"0x"`.
    pub fn from_hex_str<S: AsRef<str>>(s: S) -> Result<Self, Secp256k1Error> {
        let hex_str = {
            let s = s.as_ref();
            if s.starts_with("0x") {
                &s[2..]
            } else {
                s
            }
        };
        let key = SecretKey::from_str(hex_str)?;

        Ok(PrivateKey(key))
    }

    /// Gets the public address for a given private key.
    pub fn public_address(&self) -> Address {
        todo!()
    }
}

impl FromStr for PrivateKey {
    type Err = Secp256k1Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        PrivateKey::from_hex_str(s)
    }
}

impl Deref for PrivateKey {
    type Target = SecretKey;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Debug for PrivateKey {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "PrivateKey(********)")
    }
}

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
        Password(password.into().into())
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
        write!(f, "Password(********)")
    }
}
