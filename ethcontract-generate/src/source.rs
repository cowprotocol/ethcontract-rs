//! Allows loading serialized artifacts from various sources.
//!
//! This module does not provide means for parsing artifacts. For that,
//! use facilities in [`ethcontract_common::artifact`].
//!
//! # Examples
//!
//! Load artifact from local file:
//!
//! ```no_run
//! # use ethcontract_generate::Source;
//! let json = Source::local("build/contracts/IERC20.json")
//!     .artifact_json()
//!     .expect("failed to load an artifact");
//! ```
//!
//! Load artifact from an NPM package:
//!
//! ```no_run
//! # use ethcontract_generate::Source;
//! let json = Source::npm("npm:@openzeppelin/contracts@2.5.0/build/contracts/IERC20.json")
//!     .artifact_json()
//!     .expect("failed to load an artifact");
//! ```

use crate::util;
use anyhow::{anyhow, Context, Error, Result};
use ethcontract_common::Address;
use std::borrow::Cow;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use url::Url;

/// A source of an artifact JSON.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Source {
    /// File on the local file system.
    Local(PathBuf),

    /// Resource in the internet, available via HTTP(S).
    Http(Url),

    /// An address of a mainnet contract, available via [Etherscan].
    ///
    /// Artifacts loaded from etherstan can be parsed using
    /// the [truffle loader].
    ///
    /// Note that Etherscan rate-limits requests to their API, to avoid this,
    /// provide an Etherscan API key via the `ETHERSCAN_API_KEY`
    /// environment variable.
    ///
    /// [Etherscan]: etherscan.io
    /// [truffle loader]: ethcontract_common::artifact::truffle::TruffleLoader
    Etherscan(Address),

    /// The package identifier of an NPM package with a path to an artifact
    /// or ABI to be retrieved from [unpkg].
    ///
    /// [unpkg]: unpkg.io
    Npm(String),
}

impl Source {
    /// Parses an artifact source from a string.
    ///
    /// This method accepts the following:
    ///
    /// - relative path to a contract JSON file on the local filesystem,
    ///   for example `build/IERC20.json`. This relative path is rooted
    ///   in the current working directory. To specify the root for relative
    ///   paths, use [`with_root`] function;
    ///
    /// - absolute path to a contract JSON file on the local filesystem,
    ///   or a file URL, for example `/build/IERC20.json`, or the same path
    ///   using URL: `file:///build/IERC20.json`;
    ///
    /// - an HTTP(S) URL pointing to artifact JSON or contract ABI JSON;
    ///
    /// - a URL with `etherscan` scheme and a mainnet contract address.
    ///   For example `etherscan:0xC02AA...`. Alternatively, specify
    ///   an [etherscan] URL: `https://etherscan.io/address/0xC02AA...`.
    ///   The contract artifact or ABI will be retrieved through [`Etherscan`];
    ///
    /// - a URL with `npm` scheme, NPM package name, an optional version
    ///   and a path (defaulting to the latest version and `index.js`).
    ///   For example `npm:@openzeppelin/contracts/build/contracts/IERC20.json`.
    ///   The contract artifact or ABI will be retrieved through [`unpkg`].
    ///
    /// [Etherscan]: etherscan.io
    /// [unpkg]: unpkg.io
    pub fn parse(source: &str) -> Result<Self> {
        let root = env::current_dir()?.canonicalize()?;
        Source::with_root(root, source)
    }

    /// Parses an artifact source from a string and uses the specified root
    /// directory for resolving relative paths. See [`parse`] for more details
    /// on supported source strings.
    pub fn with_root(root: impl AsRef<Path>, source: &str) -> Result<Self> {
        let base = Url::from_directory_path(root)
            .map_err(|_| anyhow!("root path '{}' is not absolute"))?;
        let url = base.join(source.as_ref())?;

        match url.scheme() {
            "file" => Ok(Source::local(url.path())),
            "http" | "https" => match url.host_str() {
                Some("etherscan.io") => Source::etherscan(
                    url.path()
                        .rsplit('/')
                        .next()
                        .ok_or_else(|| anyhow!("HTTP URL does not have a path"))?,
                ),
                _ => Ok(Source::Http(url)),
            },
            "etherscan" => Source::etherscan(url.path()),
            "npm" => Ok(Source::npm(url.path())),
            _ => Err(anyhow!("unsupported URL '{}'", url)),
        }
    }

    /// Creates a local filesystem source from a path string.
    pub fn local(path: impl AsRef<Path>) -> Self {
        Source::Local(path.as_ref().into())
    }

    /// Creates an HTTP source from a URL.
    pub fn http(url: &str) -> Result<Self> {
        Ok(Source::Http(Url::parse(url)?))
    }

    /// Creates an [Etherscan] source from contract address on mainnet.
    ///
    /// [Etherscan]: etherscan.io
    pub fn etherscan(address: &str) -> Result<Self> {
        util::parse_address(address)
            .context("failed to parse address for Etherscan source")
            .map(Source::Etherscan)
    }

    /// Creates an NPM source from a package path.
    pub fn npm(package_path: impl Into<String>) -> Self {
        Source::Npm(package_path.into())
    }

    /// Retrieves the source JSON of the artifact.
    ///
    /// This will either read the JSON from the file system or retrieve
    /// a contract ABI from the network, depending on the source type.
    ///
    /// Contract ABIs will be wrapped into a JSON object, so that you can load
    /// them using the [truffle loader].
    ///
    /// [truffle loader]: ethcontract_common::artifact::truffle::TruffleLoader
    pub fn artifact_json(&self) -> Result<String> {
        match self {
            Source::Local(path) => get_local_contract(path),
            Source::Http(url) => get_http_contract(url),
            Source::Etherscan(address) => get_etherscan_contract(*address),
            Source::Npm(package) => get_npm_contract(package),
        }
    }
}

impl FromStr for Source {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Source::parse(s)
    }
}

fn get_local_contract(path: &Path) -> Result<String> {
    let path = if path.is_relative() {
        let absolute_path = path.canonicalize().with_context(|| {
            format!(
                "unable to canonicalize file from working dir {} with path {}",
                env::current_dir()
                    .map(|cwd| cwd.display().to_string())
                    .unwrap_or_else(|err| format!("??? ({})", err)),
                path.display(),
            )
        })?;
        Cow::Owned(absolute_path)
    } else {
        Cow::Borrowed(path)
    };

    let json = fs::read_to_string(path).context("failed to read artifact JSON file")?;
    Ok(abi_or_artifact(json))
}

fn get_http_contract(url: &Url) -> Result<String> {
    let json = util::http_get(url.as_str())
        .with_context(|| format!("failed to retrieve JSON from {}", url))?;
    Ok(abi_or_artifact(json))
}

fn get_etherscan_contract(address: Address) -> Result<String> {
    // NOTE: We do not retrieve the bytecode since deploying contracts with the
    //   same bytecode is unreliable as the libraries have already linked and
    //   probably don't reference anything when deploying on other networks.

    let api_key = env::var("ETHERSCAN_API_KEY")
        .map(|key| format!("&apikey={}", key))
        .unwrap_or_default();

    let abi_url = format!(
        "http://api.etherscan.io/api\
         ?module=contract&action=getabi&address={:?}&format=raw{}",
        address, api_key,
    );
    let abi = util::http_get(&abi_url).context("failed to retrieve ABI from Etherscan.io")?;

    // NOTE: Wrap the retrieved ABI in an empty contract, this is because
    //   currently, the code generation infrastructure depends on having an
    //   `Artifact` instance.
    let json = format!(
        r#"{{"abi":{},"networks":{{"1":{{"address":"{:?}"}}}}}}"#,
        abi, address,
    );

    Ok(json)
}

fn get_npm_contract(package: &str) -> Result<String> {
    let unpkg_url = format!("https://unpkg.com/{}", package);
    let json = util::http_get(&unpkg_url)
        .with_context(|| format!("failed to retrieve JSON from for npm package {}", package))?;

    Ok(abi_or_artifact(json))
}

/// A best-effort coercion of an ABI or an artifact JSON document into an
/// artifact JSON document.
///
/// This method uses the fact that ABIs are arrays and artifacts are
/// objects to guess at what type of document this is. Note that no parsing or
/// validation is done at this point as the document gets parsed and validated
/// at generation time.
///
/// This needs to be done as currently the contract generation infrastructure
/// depends on having an artifact.
// TODO(taminomara): add loader for plain ABIs?
fn abi_or_artifact(json: String) -> String {
    if json.trim().starts_with('[') {
        format!(r#"{{"abi":{}}}"#, json.trim())
    } else {
        json
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_source() {
        let root = "/rooted";
        for (url, expected) in &[
            (
                "relative/Contract.json",
                Source::local("/rooted/relative/Contract.json"),
            ),
            (
                "/absolute/Contract.json",
                Source::local("/absolute/Contract.json"),
            ),
            (
                "https://my.domain.eth/path/to/Contract.json",
                Source::http("https://my.domain.eth/path/to/Contract.json").unwrap(),
            ),
            (
                "etherscan:0x0001020304050607080910111213141516171819",
                Source::etherscan("0x0001020304050607080910111213141516171819").unwrap(),
            ),
            (
                "https://etherscan.io/address/0x0001020304050607080910111213141516171819",
                Source::etherscan("0x0001020304050607080910111213141516171819").unwrap(),
            ),
            (
                "npm:@openzeppelin/contracts@2.5.0/build/contracts/IERC20.json",
                Source::npm("@openzeppelin/contracts@2.5.0/build/contracts/IERC20.json"),
            ),
        ] {
            let source = Source::with_root(root, url).unwrap();
            assert_eq!(source, *expected);
        }
    }
}
