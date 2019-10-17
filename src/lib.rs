//! TODO(nlordell): documentation with examples.

pub use ethcontract_runtime as runtime;

// TODO(nlordell): re-export some useful types here: candidates are the some of
//   the `ethereum-types` types as well as some of the contract call and
//   transaction configuration types

pub use ethcontract_derive::contract;

pub mod example {
    //! An example of a derived contract for documentation purposes in roder to
    //! illustrate what the generated API. This module should not be used and is
    //! should only be included when generating documentation.

    super::contract!("../examples/WETH9.json");
}

#[cfg(test)]
mod tests {
    #[test]
    fn foo() {
        assert_eq!(crate::example::foo(), "");
    }
}
