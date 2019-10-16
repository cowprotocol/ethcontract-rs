use serde::Deserialize;
use serde_json::Error as JsonError;
use std::collections::HashMap;
use std::fs::File;
use std::io::Error as IoError;
use std::path::Path;
use thiserror::Error;
use web3::types::Address;

pub use ethabi::Contract as Abi;

#[derive(Debug, Deserialize)]
pub struct Artifact {
    pub abi: Abi,
    pub networks: HashMap<String, Network>,
}

impl Artifact {
    pub fn from_json<S>(json: S) -> Result<Artifact, ArtifactError>
    where
        S: AsRef<str>,
    {
        let artifact = serde_json::from_str(json.as_ref())?;
        Ok(artifact)
    }

    pub fn load<P>(path: P) -> Result<Artifact, ArtifactError>
    where
        P: AsRef<Path>,
    {
        let json = File::open(path)?;
        let artifact = serde_json::from_reader(json)?;
        Ok(artifact)
    }
}

#[derive(Debug, Deserialize)]
pub struct Network {
    pub address: Address,
}

#[derive(Debug, Error)]
pub enum ArtifactError {
    #[error("failed to open contract artifact file")]
    Io(#[from] IoError),

    #[error("failed to parse contract artifact JSON")]
    Json(#[from] JsonError),
}
