ethcontract::contract!("examples/truffle/build/contracts/RustCoin.json");

#[rustversion::since(1.39)]
fn main() {
    futures::executor::block_on(async {

    });
}

#[rustversion::before(1.39)]
fn main() {
    eprintln!("Rust version ^1.39 required for async/await support.");
    std::process::exit(-1);
}
