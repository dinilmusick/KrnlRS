//! Example uses no_std-compatible KrnlRS portable contracts with only alloc.
use krnlrs::portable::{Identifier, LifecycleState, OperationContract, PortableLifecycle};

fn main() {
    let mut lifecycle = PortableLifecycle::new();
    lifecycle.transition(LifecycleState::Initializing).unwrap();
    lifecycle.transition(LifecycleState::Ready).unwrap();
    let contract = OperationContract {
        name: Identifier::parse("tick").unwrap(),
        inputs: vec![Identifier::parse("millis").unwrap()],
        output: Identifier::parse("result").unwrap(),
    };
    assert_eq!(contract.name.as_str(), "tick");
}
