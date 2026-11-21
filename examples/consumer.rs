//! Compileable independent public API consumer.
use krnlrs::protocol::{MemoryTransport, Response, ServiceProxy};
use krnlrs::registry::{ServiceDescriptor, ServiceRegistry};
use krnlrs::{Functionality, Service};

fn main() {
    let service = Service::new(|input: i32| -> Result<i32, &'static str> { Ok(input + 1) });
    service.start().unwrap();
    assert_eq!(service.invoke(2), Ok(3));
    service.stop().unwrap();
    service.dispose().unwrap();

    let mut registry = ServiceRegistry::new();
    registry.register(ServiceDescriptor::new("increment", "1")).unwrap();
    assert_eq!(registry.resolve_order("increment").unwrap(), vec!["increment"]);

    let proxy = ServiceProxy::new(
        "increment",
        MemoryTransport(|request| -> Result<Response, &'static str> {
            Ok(Response { body: request.body, correlation_id: request.correlation_id })
        }),
    ).unwrap();
    let output = proxy.invoke("execute", vec![3], None).unwrap();
    assert_eq!(output.body, vec![3]);
    let mut callable = |input: i32| -> Result<i32, &'static str> { Ok(input + 1) };
    assert_eq!(Functionality::execute(&mut callable, 10), Ok(11));
}
