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

/// Shortcut for [`address_for`]`("Alice")`.
///
/// # Examples
///
/// ```
/// # use ethcontract_mock::utils::address;
/// let address = address();
/// # assert_eq!(address, "0xbf0b5a4099f0bf6c8bc4252ebec548bae95602ea".parse().unwrap());
/// ```
pub fn address() -> Address {
    address_for("Alice")
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

/// Shortcut for [`account_for`]`("Alice")`.
///
/// # Examples
///
/// ```
/// # use ethcontract_mock::utils::account;
/// let account = account();
/// # assert_eq!(account.address(), "0xbf0b5a4099f0bf6c8bc4252ebec548bae95602ea".parse().unwrap());
/// ```
pub fn account() -> Account {
    account_for("Alice")
}

/// Deploy a mocked version of a generated contract.
///
/// # Parameters
///
/// - `mock`: a [Mock] instance.
/// - `contract` type of the contract.
///
/// # Examples
///
/// ```
/// # use ethcontract_mock::{Mock, mock_contract};
/// # ethcontract::contract!(
/// #     "../examples/truffle/build/contracts/IERC20.json",
/// #     contract = IERC20 as ERC20,
/// # );
/// # fn main() {
/// let mock = Mock::new(1234);
/// let (contract, instance) = mock_contract!(mock, ERC20);
/// # }
/// ```
///
/// [Mock]: crate::Mock
#[macro_export]
macro_rules! mock_contract {
    ($mock:ident, $contract:ident) => {
        {
            let mock = $mock;
            let contract = mock.deploy($contract::raw_contract().abi.clone());
            let instance = $contract::at(&contract.web3(), contract.address());

            (contract, instance)
        }
    };
}
