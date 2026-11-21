//! Protocol-neutral request/response types and host-provided transport boundary.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub service: String,
    pub operation: String,
    pub correlation_id: Option<String>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub body: Vec<u8>,
    pub correlation_id: Option<String>,
}

pub trait ServiceTransport {
    type Error;
    fn send(&self, request: Request) -> Result<Response, Self::Error>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum ProxyError<E> {
    InvalidService,
    InvalidOperation,
    Transport(E),
}

pub struct ServiceProxy<T> {
    pub service: String,
    pub transport: T,
}

impl<T: ServiceTransport> ServiceProxy<T> {
    pub fn new(service: impl Into<String>, transport: T) -> Result<Self, ProxyError<T::Error>> {
        let service = service.into();
        if service.trim().is_empty() {
            return Err(ProxyError::InvalidService);
        }
        Ok(Self { service, transport })
    }

    pub fn invoke(
        &self,
        operation: &str,
        body: Vec<u8>,
        correlation_id: Option<String>,
    ) -> Result<Response, ProxyError<T::Error>> {
        if operation.trim().is_empty() {
            return Err(ProxyError::InvalidOperation);
        }
        self.transport
            .send(Request {
                service: self.service.clone(),
                operation: operation.into(),
                body,
                correlation_id,
            })
            .map_err(ProxyError::Transport)
    }
}

/// Embedded, in-memory host adapter. Network protocols live outside the kernel.
pub struct MemoryTransport<F>(pub F);

impl<F, E> ServiceTransport for MemoryTransport<F>
where
    F: Fn(Request) -> Result<Response, E>,
{
    type Error = E;

    fn send(&self, request: Request) -> Result<Response, E> {
        (self.0)(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_passes_correlation_and_propagates_host_errors() {
        let host = MemoryTransport(|request: Request| -> Result<Response, &'static str> {
            if request.operation == "fail" {
                Err("network")
            } else {
                Ok(Response {
                    body: request.body,
                    correlation_id: request.correlation_id,
                })
            }
        });
        let proxy = ServiceProxy::new("clock", host).unwrap();
        assert_eq!(
            proxy.invoke("tick", vec![1, 2], Some("trace-1".into())).unwrap(),
            Response {
                body: vec![1, 2],
                correlation_id: Some("trace-1".into())
            }
        );
        assert_eq!(proxy.invoke("fail", vec![], None), Err(ProxyError::Transport("network")));
        assert_eq!(proxy.invoke(" ", vec![], None), Err(ProxyError::InvalidOperation));
    }
}
