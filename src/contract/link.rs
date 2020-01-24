//! This module implements typed linking for contracts.

use crate::contract::deploy::Deploy;
use crate::errors::LinkerError;
use ethcontract_common::abi::ErrorKind as AbiErrorKind;
use ethcontract_common::Bytecode;
use std::collections::{HashMap};
use std::marker::PhantomData;
use web3::api::Web3;
use web3::contract::tokens::Tokenize;
use web3::types::Address;
use web3::Transport;

/// A trait that is implemented by a library instance and can used for linking.
pub trait LibraryInstance {
    /// The name of the library.
    fn name(&self) -> &str;

    /// The address of the library.
    fn address(&self) -> Address;
}

impl<T> LibraryInstance for &'_ T
where
    T: LibraryInstance,
{
    #[inline(always)]
    fn name(&self) -> &str {
        <T as LibraryInstance>::name(self)
    }

    #[inline(always)]
    fn address(&self) -> Address {
        <T as LibraryInstance>::address(self)
    }
}

/// A trait that is implemented by a library type that can be deployed alongside
/// a contract when deploying the contract with its libraries.
pub trait DeployLibrary {
    /// Retrieve the name of the library.
    fn name() -> &'static str;

    /// Retrieve the library's bytecode.
    fn bytecode() -> &'static Bytecode;
}

/// A marker trait that indicates that a library of type `L` can be linked with
/// the current `Deploy`.
///
/// This marker trait is intended to be used by the generated code to for
/// type-safe linking. This allows contracts to mark all libraries that can be
/// safely linked to and thus make it impossible to link a contract with a
/// library that it does not need.
pub trait DependsOn<L> {}

/// A library included in the linker.
#[derive(Clone, Debug)]
enum Library {
    /// A library that is already deployed and has a known address.
    Resolved(Address),
    /// A library that is not yet deployed and must be deployed before the
    /// contract so its address may be determined and linked into the contract
    /// bytecode.
    Pending(&'static Bytecode),
}

/// Builder for specifying options for deploying a linked contract.
#[derive(Clone, Debug)]
#[must_use = "linkers do nothing unless you `.build()` them"]
pub struct Linker<T, I>
where
    T: Transport,
    I: Deploy<T>,
{
    /// The web3 instance that will be used for deployment.
    web3: Web3<T>,
    /// The contract factory context.
    context: I::Context,
    /// The contract bytecode that is being linked.
    contract_bytecode: Bytecode,
    /// Encoded contructor parameters that get appended to the contract bytecode
    /// once linking is complete.
    encoded_contructor_params: Vec<u8>,
    /// The libraries added to the linker that need to be linked into the
    /// contract bytecode.
    libraries: Vec<(String, Library)>,
    _instance: PhantomData<I>,
}

impl<T, I> Linker<T, I>
where
    T: Transport,
    I: Deploy<T>,
{
    /// Create a new deploy builder from a `web3` provider, artifact data and
    /// deployment (constructor) parameters.
    pub fn new<P>(web3: Web3<T>, context: I::Context, params: P) -> Result<Self, LinkerError>
    where
        P: Tokenize,
    {
        let contract_bytecode = {
            let bytecode = I::bytecode(&context);
            if bytecode.is_empty() {
                return Err(LinkerError::EmptyBytecode);
            }
            bytecode.clone()
        };

        let encoded_contructor_params = {
            let params = params.into_tokens();
            match (I::abi(&context).constructor(), params.is_empty()) {
                (None, false) => return Err(AbiErrorKind::InvalidData.into()),
                (None, true) => Vec::new(),
                (Some(ctor), _) => ctor.encode_input(Vec::new(), &params)?,
            }
        };

        let libraries = Vec::new();

        Ok(Linker {
            web3,
            context,
            contract_bytecode,
            encoded_contructor_params,
            libraries,
            _instance: PhantomData,
        })
    }

    /// Adds a library instance to the linker.
    pub fn link_library<L>(self, library: L) -> Self
    where
        L: LibraryInstance,
        I: DependsOn<L>,
    {
        self.link_library_at(library.name(), library.address())
    }

    /// Adds a library to the linker by name and address.
    pub fn link_library_at<S>(self, name: S, address: Address) -> Self
    where
        S: AsRef<str>,
    {
        self.add_library(name.as_ref(), Library::Resolved(address))
    }

    /// Adds a library to deploy.
    pub fn deploy_library<L>(self) -> Self
    where
        L: DeployLibrary,
        I: DependsOn<L>,
    {
        self.add_library(L::name(), Library::Pending(L::bytecode()))
    }

    /// Add a library to the current dependency graph.
    ///
    /// Note that this method always succeeds, this is because we do not link
    /// incrementally, but rather when `build` is called so that linking errors
    /// only need to be handled in one place.
    fn add_library(mut self, name: &str, library: Library) -> Self {
        let name = name.to_owned();
        self.libraries.push((name, library));
        self
    }

    /// Links the libraries and binaries together and returns a verified
    /// deployment.
    ///
    /// This method will return an error if it finds unresolved or unused
    /// libraries during the linking process.
    pub fn build(mut self) -> Result<Deployment, LinkerError> {
        let mut pending_libraries = HashMap::new();
        for (name, library) in self.libraries {
            match library {
                Library::Resolved(address) => self.contract_bytecode.link(&name, address)?,
                Library::Pending(bytecode) => {
                    // NOTE: Check that the map doesn't contain the library
                    //   first because inserting moves `name` into the map.
                    if pending_libraries.contains_key(&name) {
                        return Err(LinkerError::UnusedDependency(name));
                    }
                    pending_libraries.insert(name, bytecode);
                }
            }
        }

        let mut libraries_to_deploy = Vec::new();
        for library in self.contract_bytecode.undefined_libraries() {
            if let Some((name, bytecode)) = pending_libraries.remove_entry(library) {
                let bytes = match bytecode.try_to_bytes() {
                    Some(bytes) => bytes,
                    None => return Err(LinkerError::NestedDependencies(name)),
                };
                libraries_to_deploy.push((name, bytes));
            } else {
                return Err(LinkerError::MissingDependency(library.to_owned()));
            }
        }

        // NOTE: At this point, the contract bytecode should be completely
        //   linkable, as we linked all the library instance addresses and
        //   verfied that the remaining dependencies are to be deployed. The
        //   libraries remaning in `pending_libraries` are extra uneeded
        //   dependencies. Report an error with the first unused dependency.
        if let Some(unused_dependency) = pending_libraries.keys().next() {
            return Err(LinkerError::UnusedDependency(unused_dependency.to_owned()));
        }

        Ok(Deployment {
            libraries: libraries_to_deploy,
            contract: (self.contract_bytecode, self.encoded_contructor_params),
        })
    }

    /// Links the libraries and binaries together and returns a `DeployBuilder`
    /// to setup the transaction for deploying the contract and its libraries.
    ///
    /// This method will return an error if it finds unresolved or unused
    /// libraries during the linking process.
    pub fn deploy(self) -> Result<(), LinkerError> {
        todo!()
    }
}

/// A full deployment of a contract including required libraries that must be
/// deployed before the contract.
#[derive(Clone, Debug)]
pub struct Deployment {
    libraries: Vec<(String, Vec<u8>)>,
    contract: (Bytecode, Vec<u8>),
}
