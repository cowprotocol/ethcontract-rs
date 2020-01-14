/// Parse a string address of the form "0x...".
///
/// # Panics
///
/// If the address is invalid.
#[macro_export]
macro_rules! addr {
    ($value:expr) => {
        $value[2..]
            .parse::<web3::types::Address>()
            .expect("valid address")
    };
}

pub use addr;

/// Parse a string uint256 of the form "0x...".
///
/// # Panics
///
/// If the uint is invalid.
#[macro_export]
macro_rules! uint {
    ($value:expr) => {
        $value[2..]
            .parse::<web3::types::U256>()
            .expect("valid address")
    };
}

pub use uint;

/// Parse a string 256-bit hash of the form "0x...".
///
/// # Panics
///
/// If the hash is invalid.
#[macro_export]
macro_rules! hash {
    ($value:expr) => {
        $value[2..]
            .parse::<web3::types::H256>()
            .expect("valid hash")
    };
}

pub use hash;

/// Parse hex encoded string of the form "0x...".
///
/// # Panics
///
/// If the hex string is invalid.
#[macro_export]
macro_rules! bytes {
    ($value:expr) => {
        serde_json::from_str::<web3::types::Bytes>(&format!("\"{}\"", $value)).expect("valid bytes")
    };
}

pub use bytes;

/// Parse a string 256-bit private key of the form "0x...".
///
/// # Panics
///
/// If the private key is invalid.
#[macro_export]
macro_rules! key {
    ($value:expr) => {
        ethsign::SecretKey::from_raw(&hash!($value)[..]).expect("valid key")
    };
}

pub use key;
