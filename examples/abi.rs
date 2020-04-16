use ethcontract::prelude::*;
use std::any;

ethcontract::contract!("examples/truffle/build/contracts/AbiTypes.json");

fn main() {
    futures::executor::block_on(run());
}

async fn run() {
    let (eloop, http) = Http::new("http://localhost:9545").expect("transport failure");
    eloop.into_remote();
    let web3 = Web3::new(http);

    let instance = AbiTypes::builder(&web3)
        .gas(4_712_388.into())
        .deploy()
        .await
        .expect("contract deployment failure");
    println!("Using contract at {:?}", instance.address());

    calls(&instance).await;
    events(&instance).await;
}

async fn calls(instance: &AbiTypes) {
    macro_rules! debug_call {
        (instance. $call:ident ()) => {{
            let value = instance
                .$call()
                .call()
                .await
                .expect(concat!(stringify!($call), " failed"));
            println!(
                "{}() -> {}\n  ⏎ {:?}",
                stringify!($call),
                type_name_of(&value),
                value,
            )
        }};
    }

    debug_call!(instance.get_void());
    debug_call!(instance.get_u8());
    debug_call!(instance.get_u16());
    debug_call!(instance.get_u32());
    debug_call!(instance.get_u64());
    debug_call!(instance.get_u128());
    debug_call!(instance.get_u256());

    debug_call!(instance.get_i8());
    debug_call!(instance.get_i16());
    debug_call!(instance.get_i32());
    debug_call!(instance.get_i64());
    debug_call!(instance.get_i128());
    debug_call!(instance.get_i256());

    debug_call!(instance.get_bool());

    debug_call!(instance.get_bytes());
    debug_call!(instance.get_fixed_bytes());
    debug_call!(instance.get_address());
    debug_call!(instance.get_string());

    debug_call!(instance.get_array());
    debug_call!(instance.get_fixed_array());
}

async fn events(instance: &AbiTypes) {
    macro_rules! debug_events {
        (instance.events(). $events:ident ()) => {{
            let events = instance
                .events()
                .$events()
                .query()
                .await
                .expect(concat!(stringify!($events), " failed"));
            let event_data = events
                .iter()
                .map(|event| event.inner_data())
                .collect::<Vec<_>>();
            println!("{}()\n  ⏎ {:?}", stringify!($events), event_data);
        }};
    }

    instance
        .emit_values()
        // NOTE: Gas estimation seems to not work for this call.
        .gas(4_712_388.into())
        .send()
        .await
        .expect("failed to emit value events");

    debug_events!(instance.events().value_uint());
    debug_events!(instance.events().value_int());
    debug_events!(instance.events().value_bool());
    debug_events!(instance.events().value_bytes());
    debug_events!(instance.events().value_array());
    debug_events!(instance.events().value_indexed());

    let all_events = instance
        .all_events()
        .query()
        .await
        .expect("failed to retrieve all events");
    for event in all_events {
        if let abi_types::Event::Values(data) = event.inner_data() {
            println!("anonymous event\n  ⏎ {:?}", data);
        }
    }
}

fn type_name_of<T>(_: &T) -> &'static str {
    any::type_name::<T>()
}
