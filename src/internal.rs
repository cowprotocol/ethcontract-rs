//! Module provides internal functionality specific to generated contracts that
//! isn't intended to be used directly but rather from generated code.

use crate::DynWeb3;
use crate::contract::{Deployments, Factory};
use crate::transport::DynTransport;
use ethcontract_common::{Abi, Bytecode};
use std::marker::PhantomData;
use web3::types::Address;

/// A struct for wrapping generated contract types. This allows these contract
/// types to implement traits that require instances without them.
pub struct Contract<C>(PhantomData<Box<C>>);

impl<C> Default for Contract<C> {
    fn default() -> Self {
        Contract(PhantomData)
    }
}

/// Analogous to the `ethcontract::contract::Deployments` trait except with
/// associated functions instead of methods meaning that no `self` instance is
/// required.
pub trait ContractDeployments: Sized {
    /// See `ethcontract::contract::Deployments::from_network`.
    fn from_network(web3: DynWeb3, network_id: &str) -> Option<Self>;
}

impl<C: ContractDeployments> Deployments<DynTransport> for Contract<C> {
    type Instance = C;

    fn from_network(self, web3: DynWeb3, network_id: &str) -> Option<Self::Instance> {
        C::from_network(web3, network_id)
    }
}

/// Analogous to the `ethcontract::contract::Factory` trait except with
/// associated functions instead of methods meaning that no `self` instance is
/// required.
pub trait ContractFactory: Sized {
    /// See `ethcontract::contract::Factory::bytecode`.
    fn bytecode() -> &'static Bytecode;

    /// See `ethcontract::contract::Factory::abi`.
    fn abi() -> &'static Abi;

    /// See `ethcontract::contract::Factory::at_address`.
    fn at_address(web3: DynWeb3, address: Address) -> Self;
}

impl<C: ContractFactory> Factory<DynTransport> for Contract<C> {
    type Instance = C;

    fn bytecode(&self) -> &Bytecode {
        C::bytecode()
    }

    fn abi(&self) -> &Abi {
        C::abi()
    }

    fn at_address(self, web3: DynWeb3, address: Address) -> Self::Instance {
        C::at_address(web3, address)
    }
}
