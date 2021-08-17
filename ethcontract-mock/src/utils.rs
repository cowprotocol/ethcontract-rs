//! Convenience utilities for tests.

use ethcontract::{Account, Address, PrivateKey};

/// Generate public address by hashing the given string.
///
/// # Safety
///
/// This function is intended for tests and should not be used in production.
///
/// # Examples
///
/// ```
/// # use ethcontract_mock::utils::address_for;
/// let address = address_for("Alice");
/// # assert_eq!(address, "0xbf0b5a4099f0bf6c8bc4252ebec548bae95602ea".parse().unwrap());
/// ```
pub fn address_for(who: &str) -> Address {
    account_for(who).address()
}

/// Generate a private key by hashing the given string.
///
/// # Safety
///
/// This function is intended for tests and should not be used in production.
///
/// # Examples
///
/// ```
/// # use ethcontract_mock::utils::account_for;
/// let account = account_for("Bob");
/// # assert_eq!(account.address(), "0x4dba461ca9342f4a6cf942abd7eacf8ae259108c".parse().unwrap());
/// ```
pub fn account_for(who: &str) -> Account {
    use ethcontract::web3::signing::keccak256;
    Account::Offline(
        PrivateKey::from_raw(keccak256(who.as_bytes())).unwrap(),
        None,
    )
}
