//! Compileable independent public API consumer.
use krnlrs::facade::{InvocationOptions, NoopHooks, ServiceFacade};
use krnlrs::queue::QueueOptions;
use krnlrs::protocol::{MemoryTransport, Request, Response, ServiceProxy};
use krnlrs::registry::{ServiceDescriptor, ServiceRegistry};
use krnlrs::{Functionality, Service};

fn main() {
    let service = Service::new(|input: i32| -> Result<i32, &'static str> { Ok(input + 1) });
    service.start().unwrap();
    assert_eq!(service.invoke(2), Ok(3));
    service.stop().unwrap();
    service.dispose().unwrap();

    let mut registry = ServiceRegistry::new();
    registry
        .register(ServiceDescriptor::new("increment", "1"))
        .unwrap();
    assert_eq!(
        registry.resolve_order("increment").unwrap(),
        vec!["increment"]
    );

    let proxy = ServiceProxy::new(
        "increment",
        MemoryTransport(|request: Request| -> Result<Response, &'static str> {
            Ok(Response {
                body: request.body,
                correlation_id: request.correlation_id,
            })
        }),
    )
    .unwrap();
    let output = proxy.invoke("execute", vec![3], None).unwrap();
    assert_eq!(output.body, vec![3]);
    let mut callable = |input: i32| -> Result<i32, &'static str> { Ok(input + 1) };
    assert_eq!(Functionality::execute(&mut callable, 10), Ok(11));
    let runtime = ServiceFacade::new(
        ServiceDescriptor::new("counter", "1"),
        NoopHooks,
        QueueOptions::default(),
        4,
    ).unwrap();
    runtime.start().unwrap();
    let counted = runtime.invoke(
        "increment",
        41i32,
        InvocationOptions::default(),
        |_| Ok::<_, &'static str>(()),
        |value, context| {
            assert!(!context.is_cancelled());
            Ok::<_, &'static str>(value + 1)
        },
        |_| Ok::<_, &'static str>(()),
    );
    assert_eq!(counted, Ok(42));
    runtime.stop().unwrap();
    runtime.restart().unwrap();
    runtime.dispose().unwrap();
    assert!(runtime.start().is_err());

}
