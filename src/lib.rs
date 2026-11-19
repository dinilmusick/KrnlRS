//! Portable KrnlTS service execution and lifecycle contracts.
//! Service-owned execution only; cross-service orchestration belongs to Skipo.
//! No Kloks, Floks, Skipo, or Krusbl dependencies.
#![forbid(unsafe_code)]

use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    Created,
    Running,
    Stopped,
    Disposed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceError<E> {
    NotRunning,
    Disposed,
    Poisoned,
    Execution(E),
}

/// A single service's typed functionality. Implementations own their validation
/// and return explicit errors; no cross-service dispatch is performed here.
pub trait Functionality<I, O, E>: Send {
    fn execute(&mut self, input: I) -> Result<O, E>;
}

impl<I, O, E, F> Functionality<I, O, E> for F
where
    F: FnMut(I) -> Result<O, E> + Send,
{
    fn execute(&mut self, input: I) -> Result<O, E> {
        self(input)
    }
}

/// Serialized, thread-safe service boundary with restartable stop and terminal
/// idempotent disposal. A mutex protects state and prevents stop/dispose from
/// overtaking an in-flight execution.
pub struct Service<F> {
    inner: Mutex<(Lifecycle, F)>,
}

impl<F> Service<F> {
    pub fn new(functionality: F) -> Self {
        Self {
            inner: Mutex::new((Lifecycle::Created, functionality)),
        }
    }

    pub fn lifecycle(&self) -> Result<Lifecycle, ServiceError<()>> {
        self.inner
            .lock()
            .map(|guard| guard.0)
            .map_err(|_| ServiceError::Poisoned)
    }

    /// Start or restart a stopped service; never resurrect a disposed service.
    pub fn start(&self) -> Result<(), ServiceError<()>> {
        let mut guard = self.inner.lock().map_err(|_| ServiceError::Poisoned)?;
        if guard.0 == Lifecycle::Disposed {
            return Err(ServiceError::Disposed);
        }
        guard.0 = Lifecycle::Running;
        Ok(())
    }

    /// Stop is idempotent and restartable until disposal.
    pub fn stop(&self) -> Result<(), ServiceError<()>> {
        let mut guard = self.inner.lock().map_err(|_| ServiceError::Poisoned)?;
        if guard.0 == Lifecycle::Disposed {
            return Err(ServiceError::Disposed);
        }
        guard.0 = Lifecycle::Stopped;
        Ok(())
    }

    /// Disposal is terminal and idempotent. No execution may begin afterward.
    pub fn dispose(&self) -> Result<(), ServiceError<()>> {
        let mut guard = self.inner.lock().map_err(|_| ServiceError::Poisoned)?;
        guard.0 = Lifecycle::Disposed;
        Ok(())
    }

    pub fn invoke<I, O, E>(&self, input: I) -> Result<O, ServiceError<E>>
    where
        F: Functionality<I, O, E>,
    {
        let mut guard = self.inner.lock().map_err(|_| ServiceError::Poisoned)?;
        match guard.0 {
            Lifecycle::Disposed => return Err(ServiceError::Disposed),
            Lifecycle::Running => {}
            Lifecycle::Created | Lifecycle::Stopped => return Err(ServiceError::NotRunning),
        }
        guard.1.execute(input).map_err(ServiceError::Execution)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn lifecycle_is_restartable_but_disposal_is_terminal() {
        let service = Service::new(|x: i32| Ok::<_, &'static str>(x * 2));
        assert_eq!(service.lifecycle(), Ok(Lifecycle::Created));
        assert_eq!(service.invoke(2), Err(ServiceError::NotRunning));
        service.start().unwrap();
        assert_eq!(service.invoke(2), Ok(4));
        service.stop().unwrap();
        service.stop().unwrap();
        assert_eq!(service.invoke(2), Err(ServiceError::NotRunning));
        service.start().unwrap();
        assert_eq!(service.invoke(3), Ok(6));
        service.dispose().unwrap();
        service.dispose().unwrap();
        assert_eq!(service.lifecycle(), Ok(Lifecycle::Disposed));
        assert_eq!(service.start(), Err(ServiceError::Disposed));
        assert_eq!(service.stop(), Err(ServiceError::Disposed));
        assert_eq!(service.invoke(3), Err(ServiceError::Disposed));
    }

    #[test]
    fn functionality_errors_are_not_silently_swallowed() {
        let service = Service::new(|_: ()| Err::<(), _>("invalid input"));
        service.start().unwrap();
        assert_eq!(service.invoke(()), Err(ServiceError::Execution("invalid input")));
    }

    #[test]
    fn concurrent_calls_are_serialized_and_disposal_prevents_new_calls() {
        let service = Arc::new(Service::new({
            let mut counter = 0usize;
            move |_: ()| {
                counter += 1;
                Ok::<_, ()>(counter)
            }
        }));
        service.start().unwrap();
        let handles: Vec<_> = (0..16)
            .map(|_| {
                let service = Arc::clone(&service);
                thread::spawn(move || service.invoke(()).unwrap())
            })
            .collect();
        let mut values: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        values.sort_unstable();
        assert_eq!(values, (1..=16).collect::<Vec<_>>());
        service.dispose().unwrap();
        assert_eq!(service.invoke(()), Err(ServiceError::Disposed));
    }
}
