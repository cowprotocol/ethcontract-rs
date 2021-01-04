/// Parse a string address of the form "0x...".
///
/// # Panics
///
/// If the address is invalid.
macro_rules! addr {
    ($value:expr) => {
        $value[2..]
            .parse::<web3::types::Address>()
            .expect("invalid address")
    };
}

/// Parse a string uint256 of the form "0x...".
///
/// # Panics
///
/// If the uint is invalid.
macro_rules! uint {
    ($value:expr) => {
        $value[2..]
            .parse::<web3::types::U256>()
            .expect("invalid address")
    };
}

/// Parse a string 256-bit hash of the form "0x...".
///
/// # Panics
///
/// If the hash is invalid.
macro_rules! hash {
    ($value:expr) => {
        $value[2..]
            .parse::<web3::types::H256>()
            .expect("invalid hash")
    };
}

/// Parse hex encoded string of the form "0x...".
///
/// # Panics
///
/// If the hex string is invalid.
macro_rules! bytes {
    ($value:expr) => {
        serde_json::from_str::<web3::types::Bytes>(&format!("\"{}\"", $value))
            .expect("invalid bytes")
    };
}

/// Parse a string 256-bit private key of the form "0x...".
///
/// # Panics
///
/// If the private key is invalid.
macro_rules! key {
    ($value:expr) => {
        $crate::secret::PrivateKey::from_slice(&hash!($value)[..]).expect("invalid key")
    };
}
