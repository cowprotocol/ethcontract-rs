# `ethcontract-generate`

An alternative API for generating type-safe contract bindings from `build.rs`
scripts. Using this method instead of the procedural macro has a couple
advantages:

- proper integration with with RLS and Racer for autocomplete support;
- ability to inspect the generated code.

The downside of using the generator API is the requirement of having a build
script instead of a macro invocation.

## Getting Started

Using crate requires two dependencies - one for the runtime and one for the
generator:

```toml
[dependencies]
ethcontract = { version = "...", default-features = false }

[build-dependencies]
ethcontract-generate = "..."
```

It is recommended that both versions be kept in sync or else unexpected
behaviour may occur.

Then, in your `build.rs` include the following code:

```rust
use ethcontract_generate::loaders::TruffleLoader;
use ethcontract_generate::ContractBuilder;

fn main() {
    // Prepare filesystem paths.
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest = std::path::Path::new(&out_dir).join("rust_coin.rs");
    
    // Load a contract.
    let contract = TruffleLoader::new()
        .load_contract_from_file("../build/Contract.json")
        .unwrap();
    
    // Generate bindings for it.
    ContractBuilder::new()
        .generate(&contract)
        .unwrap()
        .write_to_file(dest)
        .unwrap();
}

```

## Relation to `ethcontract-derive`

`ethcontract-derive` uses `ethcontract-generate` under the hood so their
generated bindings should be identical, they just provide different APIs to the
same functionality.

The long term goal of this project is to maintain `ethcontract-derive`. For now
there is no extra work in having it split into two separate crates. That being
said if RLS support improves for procedural macro generated code, it is possible
that this crate be deprecated in favour of `ethcontract-derive` as long as there
is no good argument to keep it around.
