//! Module with common error types.

use serde_json::Error as JsonError;
use std::io::Error as IoError;
use thiserror::Error;

/// An error in loading or parsing a truffle artifact.
#[derive(Debug, Error)]
pub enum ArtifactError {
    /// An IO error occurred when loading a truffle artifact from disk.
    #[error("failed to open contract artifact file: {0}")]
    Io(#[from] IoError),

    /// A JSON error occurred while parsing a truffle artifact.
    #[error("failed to parse contract artifact JSON: {0}")]
    Json(#[from] JsonError),
}

/// An error reading bytecode string representation.
#[derive(Debug, Error)]
pub enum BytecodeError {
    /// Missing hex prefix at start of string.
    #[error("missing 0x hex prefix at start of bytecode")]
    MissingHexPrefix,

    /// Bytecode string is not an even length.
    #[error("invalid bytecode length")]
    InvalidLength,

    /// Placeholder is not long enough at end of bytecode string.
    #[error("placeholder at end of bytecode is too short")]
    PlaceholderTooShort,

    /// Invalid hex digit
    #[error("invalid hex digit '{0}'")]
    InvalidHexDigit(char),
}

/// An error linking a library to bytecode.
#[derive(Debug, Error)]
pub enum LinkError {
    /// Error when attempting to link a library when its placeholder cannot be
    /// found.
    #[error("unable to link library: can't find link placeholder for {0}")]
    NotFound(String),

    /// Error producing final bytecode binary when there are missing libraries
    /// that are not linked. Analogous to "undefinied symbol" error for
    /// traditional linkers.
    #[error("undefined library {0}")]
    UndefinedLibrary(String),
}
