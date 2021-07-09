# Build script example

This example shows a simple setup that uses a build script to generate contract
bindings from hardhat `deployments` directory.

We use `build.rs` to generate file `contracts.rs`. Then we include said file
into our code using the `include!` macro.
