#![allow(dead_code)]

//! This module implements typed linking for contracts.

use crate::contract::deploy::Deploy;
use crate::errors::LinkerError;
use ethcontract_common::abi::ErrorKind as AbiErrorKind;
use ethcontract_common::Bytecode;
use std::collections::{HashMap, HashSet, VecDeque};
use std::iter;
use std::marker::PhantomData;
use web3::api::Web3;
use web3::contract::tokens::Tokenize;
use web3::types::Address;
use web3::Transport;

/// A trait that is implemented by a library used for linking.
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

/// A trait that is implemented by a library type that can be deployed.
pub trait DeployLibrary {
    /// Retrieve the name of the library.
    fn name() -> &'static str;

    /// Retrieve the library's bytecode.
    fn bytecode() -> &'static Bytecode;
}

/// A marker trait that indicates that a library of type `L` can be linked with
/// the current `Deploy`.
pub trait DependsOn<L> {}

#[derive(Clone, Debug)]
enum Library {
    Resolved(Address),
    Pending(Code),
}

#[derive(Clone, Debug)]
pub enum Code {
    Linked(Vec<u8>),
    Unlinked(Bytecode),
}

impl Code {
    fn into_bytes(self) -> Option<Vec<u8>> {
        match self {
            Code::Linked(bytes) => Some(bytes),
            _ => None,
        }
    }

    fn unlinked(&self) -> Option<&Bytecode> {
        match self {
            Code::Unlinked(code) => Some(code),
            _ => None,
        }
    }

    pub fn try_link<S>(&mut self, name: S, address: Address) -> bool
    where
        S: AsRef<str>,
    {
        match self {
            Code::Unlinked(code) => {
                if code.try_link(name, address) {
                    if let Some(bytes) = code.try_to_bytes() {
                        *self = Code::Linked(bytes);
                    }
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn are_dependencies_met(&self, dependencies: &HashSet<String>) -> bool {
        self.unlinked()
            .map(|code| {
                code.undefined_libraries()
                    .all(|dep| dependencies.contains(dep))
            })
            .unwrap_or(true)
    }

    fn to_bytes(&self) -> Option<Vec<u8>> {
        self.unlinked()?.try_to_bytes()
    }
}

impl From<&'_ Bytecode> for Code {
    fn from(bytecode: &Bytecode) -> Self {
        match bytecode.to_bytes() {
            Ok(bytes) => Code::Linked(bytes),
            Err(_) => Code::Unlinked(bytecode.clone()),
        }
    }
}

/// Builder for specifying options for deploying a linked contract.
#[derive(Clone, Debug)]
#[must_use = "linkers do nothing unless you `.build()` them"]
pub struct Linker<T, I>
where
    T: Transport,
    I: Deploy<T>,
{
    web3: Web3<T>,
    context: I::Context,
    contract: Code,
    params: Vec<u8>,
    libraries: HashMap<String, Library>,
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
        let bytecode = I::bytecode(&context);
        if bytecode.is_empty() {
            return Err(LinkerError::EmptyBytecode);
        }

        let contract = bytecode.into();
        let params = {
            let params = params.into_tokens();
            match (I::abi(&context).constructor(), params.is_empty()) {
                (None, false) => return Err(AbiErrorKind::InvalidData.into()),
                (None, true) => Vec::new(),
                (Some(ctor), _) => ctor.encode_input(Vec::new(), &params)?,
            }
        };

        let libraries = HashMap::new();

        Ok(Linker {
            web3,
            context,
            contract,
            params,
            libraries,
            _instance: PhantomData,
        })
    }

    /// Links a library instance to the current dependency graph.
    pub fn link_library<L>(self, library: L) -> Result<Self, LinkerError>
    where
        L: LibraryInstance,
        I: DependsOn<L>,
    {
        self.link_library_with_name(library.name(), library.address())
    }

    /// Links a library by name and address.
    pub fn link_library_with_name<S>(self, name: S, address: Address) -> Result<Self, LinkerError>
    where
        S: AsRef<str>,
    {
        self.add_library(name.as_ref(), || Library::Resolved(address))
    }

    /// Adds a library to deploy.
    pub fn deploy_library<L>(self) -> Result<Self, LinkerError>
    where
        L: DeployLibrary,
        I: DependsOn<L>,
    {
        self.add_library(L::name(), || Library::Pending(L::bytecode().into()))
    }

    /// Add a library to the current dependency graph.
    fn add_library<F>(mut self, name: &str, library: F) -> Result<Self, LinkerError>
    where
        F: FnOnce() -> Library,
    {
        let name = name.to_owned();
        if self.libraries.get(&name).is_some() {
            return Err(LinkerError::MultipleDefinitions(name));
        }
        self.libraries.insert(name, library());

        Ok(self)
    }

    /// Links the libraries and binaries together and returns a verified
    /// deployment that is guaranteed to have all required libraries.
    ///
    /// This method will return an error if it finds unresolved or unused
    /// libraries during the linking process.
    pub fn build(mut self) -> Result<VerifiedDeployment, LinkerError> {
        // First, split the libraries into resolved libraries (libraries that
        // have addresses) and pending libraries (libraries that need to be
        // deployed alongside the contract).
        let (resolved, mut pending) = self.libraries.into_iter().fold(
            (Vec::new(), HashMap::new()),
            |(mut resolved, mut pending), (name, library)| {
                match library {
                    Library::Resolved(address) => resolved.push((name, address)),
                    Library::Pending(code) => {
                        pending.insert(name, code);
                    }
                }
                (resolved, pending)
            },
        );

        // Link all resolved libraries into the pending bytecodes. Note that we
        // also have to link libraries in case there are nested dependencies.
        for (name, address) in resolved {
            let is_unused = iter::once(&mut self.contract)
                .chain(pending.values_mut())
                .map(|code| code.try_link(&name, address))
                .all(|result| !result);

            if is_unused {
                return Err(LinkerError::UnusedDependency(name));
            }
        }

        // Verify that there are not unused pending libraries or missing
        // libraries. Note that for the missing library check, we don't need to
        // consider the resolved libraries as they have already been linked into
        // the bytecode of the contract and pending libraries.
        let remaining_deps = iter::once(&self.contract)
            .chain(pending.values())
            .filter_map(|code| code.unlinked())
            .flat_map(|code| code.undefined_libraries())
            .collect::<HashSet<_>>();

        if let Some(missing_dep) = remaining_deps
            .iter()
            .copied()
            .find(|&dep| !pending.contains_key(dep))
        {
            return Err(LinkerError::MissingDependency(missing_dep.into()));
        }

        if let Some(unused_dep) = pending
            .iter()
            .map(|(name, _)| name)
            .find(|dep| remaining_deps.contains(dep.as_str()))
        {
            return Err(LinkerError::UnusedDependency(unused_dep.into()));
        }

        // Order the pending dependencies so that libraries that have nested
        // dependencies come after their nested dependencies.
        /*
        let mut included = HashSet::new();
        let mut libraries = VecDeque::with_capacity(pending.len());
        while !pending.is_empty() {
            match pending
                .iter()
                .find(|(_, code)| code.are_dependencies_met(&included))
            {
                Some((name, _)) => {
                    let name = name.clone();
                    let code = pending.remove(&name).unwrap();

                    included.insert(name.clone());
                    libraries.push_back((name, code));
                }
                None => {
                    return Err(LinkerError::CircularDependencies(
                        pending.keys().cloned().collect(),
                    ))
                }
            }
        }
        */

        let mut remaining = Vec::new();
        let mut libraries = Vec::new();
        for 

        Ok(VerifiedDeployment {
            libraries,
            contract: self.contract,
            params: self.params,
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

#[derive(Clone, Debug)]
pub struct VerifiedDeployment {
    libraries: VecDeque<(String, Code)>,
    contract: (Code, Vec<u8>),
}

impl VerifiedDeployment {
    pub fn from_raw_contract(code: Vec<u8>) -> Self {
        VerifiedDeployment {
            libraries: VecDeque::new(),
            contract: (Code::Linked(code), Vec::new()),
        }
    }

    pub fn bytecodes(self) -> (Vec<u8>, PendingBytecodes) {
        let mut bytecodes = PendingBytecodes {
            pending_library_name: None,
            libraries: self.libraries,
            contract: Some(self.contract),
        };

        (
            bytecodes
                .pop()
            bytecodes,
        )
    }
}

#[derive(Clone, Debug)]
pub struct PendingBytecodes {
    pending_library_name: Option<String>,
    libraries: VecDeque<(String, Code)>,
    contract: Option<(Code, Vec<u8>)>,
}

impl PendingBytecodes {
    fn empty() -> Self {
        PendingBytecodes {
            pending_library_name: None,
            libraries: VecDeque::new(),
            contract: None,
        }
    }

    fn pop(&mut self) -> Vec<u8> {
        if let Some((name, library)) = self.libraries.pop_front() {
            // Library is always guaranteed to be completely linked, so this
            // should never panic.
            self.pending_library_name = Some(name);
                library
                    .into_bytes()
                    .expect("unexpected unlinked library in bytecode")
        } else if let Some((contract, params)) = self.contract.take() {
            // Contract is always guaranteed to be completely linked, so this
            // should never panic.
            let mut bytecode = contract
                .into_bytes()
                .expect("unexpected unlinked library in bytecode");
            bytecode.extend_from_slice(&params);
            bytecode
        } else {
        // A verified deployment is always guaranteed to at least have contract
        // bytes, so `pop`-ing the first bytecode from the pending bytecodes
        // will never return `None` and hence the unwrap will never panic.
            unreachable!()
        }
    }

    pub fn next(&mut self, previous_code_address: Address) -> Option<Vec<u8>> {
        if let Some(name) = self.pending_library_name.take() {
            for code in self
                .libraries
                .iter_mut()
                .map(|(_, code)| code)
                .chain(self.contract.iter_mut().map(|(code, _)| code))
            {
                code.try_link(&name, previous_code_address);
            }
        }
        Some(self.pop())
    }
}
