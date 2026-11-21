//! Portable KrnlTS service execution and lifecycle contracts.
//! Service-owned execution only; cross-service orchestration belongs to Skipo.
//! No Kloks, Floks, Skipo, or Krusbl dependencies.
#![forbid(unsafe_code)]
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

pub mod portable;

#[cfg(feature = "std")]
pub mod events;
#[cfg(feature = "std")]
pub mod facade;
#[cfg(feature = "std")]
pub mod lifecycle;
#[cfg(feature = "std")]
pub mod orchestration;
#[cfg(feature = "std")]
pub mod ownership;
#[cfg(feature = "std")]
pub mod protocol;
#[cfg(feature = "std")]
pub mod queue;
#[cfg(feature = "std")]
pub mod registry;
#[cfg(feature = "std")]
pub mod state;
#[cfg(feature = "std")]
pub mod telemetry;
#[cfg(feature = "std")]
pub mod validation;

#[cfg(feature = "std")]
mod std_api {
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
        Busy,
        Execution(E),
    }

    /// Distinguishes a rejected input from a failure during service execution.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum CheckedServiceError<V, E> {
        Lifecycle(ServiceError<()>),
        Validation(V),
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

        /// Validate before executing, under the same lifecycle lock. Invalid input
        /// never reaches functionality, and stopped/disposed services fail closed
        /// without invoking user-provided validation callbacks.
        pub fn invoke_checked<I, O, E, V>(
            &self,
            input: I,
            validate: impl FnOnce(&I) -> Result<(), V>,
        ) -> Result<O, CheckedServiceError<V, E>>
        where
            F: Functionality<I, O, E>,
        {
            let mut guard = self
                .inner
                .lock()
                .map_err(|_| CheckedServiceError::Lifecycle(ServiceError::Poisoned))?;
            match guard.0 {
                Lifecycle::Disposed => {
                    return Err(CheckedServiceError::Lifecycle(ServiceError::Disposed));
                }
                Lifecycle::Running => {}
                Lifecycle::Created | Lifecycle::Stopped => {
                    return Err(CheckedServiceError::Lifecycle(ServiceError::NotRunning));
                }
            }
            validate(&input).map_err(CheckedServiceError::Validation)?;
            guard
                .1
                .execute(input)
                .map_err(CheckedServiceError::Execution)
        }

        /// Fail-fast invocation for callers that must not wait behind an active
        /// service operation. Contention is distinct from a poisoned lock.
        /// A successful lock still enforces the current lifecycle.
        pub fn try_invoke<I, O, E>(&self, input: I) -> Result<O, ServiceError<E>>
        where
            F: Functionality<I, O, E>,
        {
            use std::sync::TryLockError;
            let mut guard = match self.inner.try_lock() {
                Ok(guard) => guard,
                Err(TryLockError::WouldBlock) => return Err(ServiceError::Busy),
                Err(TryLockError::Poisoned(_)) => return Err(ServiceError::Poisoned),
            };
            match guard.0 {
                Lifecycle::Disposed => return Err(ServiceError::Disposed),
                Lifecycle::Running => {}
                Lifecycle::Created | Lifecycle::Stopped => return Err(ServiceError::NotRunning),
            }
            guard.1.execute(input).map_err(ServiceError::Execution)
        }

        /// Nonblocking validated invocation. Contention fails before validation,
        /// and lifecycle checks run before user callbacks under the same lock.
        pub fn try_invoke_checked<I, O, E, V>(
            &self,
            input: I,
            validate: impl FnOnce(&I) -> Result<(), V>,
        ) -> Result<O, CheckedServiceError<V, E>>
        where
            F: Functionality<I, O, E>,
        {
            use std::sync::TryLockError;
            let mut guard = match self.inner.try_lock() {
                Ok(guard) => guard,
                Err(TryLockError::WouldBlock) => {
                    return Err(CheckedServiceError::Lifecycle(ServiceError::Busy));
                }
                Err(TryLockError::Poisoned(_)) => {
                    return Err(CheckedServiceError::Lifecycle(ServiceError::Poisoned));
                }
            };
            match guard.0 {
                Lifecycle::Disposed => {
                    return Err(CheckedServiceError::Lifecycle(ServiceError::Disposed));
                }
                Lifecycle::Running => {}
                Lifecycle::Created | Lifecycle::Stopped => {
                    return Err(CheckedServiceError::Lifecycle(ServiceError::NotRunning));
                }
            }
            validate(&input).map_err(CheckedServiceError::Validation)?;
            guard
                .1
                .execute(input)
                .map_err(CheckedServiceError::Execution)
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
            assert_eq!(
                service.invoke(()),
                Err(ServiceError::Execution("invalid input"))
            );
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
        #[test]
        fn checked_invocation_rejects_invalid_inputs_without_execution() {
            let service = Service::new({
                let mut calls = 0;
                move |value: i32| {
                    calls += 1;
                    Ok::<_, &'static str>((value, calls))
                }
            });
            let validate = |value: &i32| {
                if *value >= 0 {
                    Ok(())
                } else {
                    Err("negative")
                }
            };
            assert_eq!(
                service.invoke_checked(-1, validate),
                Err(CheckedServiceError::Lifecycle(ServiceError::NotRunning))
            );
            service.start().unwrap();
            assert_eq!(
                service.invoke_checked(-1, validate),
                Err(CheckedServiceError::Validation("negative"))
            );
            assert_eq!(service.invoke_checked(3, validate), Ok((3, 1)));
            service.dispose().unwrap();
            assert_eq!(
                service.invoke_checked(4, validate),
                Err(CheckedServiceError::Lifecycle(ServiceError::Disposed))
            );
        }

        #[test]
        fn try_invoke_is_nonblocking_and_preserves_lifecycle() {
            use std::sync::mpsc;
            let (entered_tx, entered_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let service = Arc::new(Service::new(move |x: i32| {
                if x == 1 {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                }
                Ok::<_, ()>(x)
            }));
            assert_eq!(service.try_invoke(0), Err(ServiceError::NotRunning));
            service.start().unwrap();
            let running = Arc::clone(&service);
            let worker = thread::spawn(move || running.invoke(1));
            entered_rx.recv().unwrap();
            assert_eq!(service.try_invoke(2), Err(ServiceError::Busy));
            release_tx.send(()).unwrap();
            assert_eq!(worker.join().unwrap(), Ok(1));
            assert_eq!(service.try_invoke(3), Ok(3));
            service.stop().unwrap();
            assert_eq!(service.try_invoke(4), Err(ServiceError::NotRunning));
            service.dispose().unwrap();
            assert_eq!(service.try_invoke(5), Err(ServiceError::Disposed));
        }

        #[test]
        fn checked_invocation_preserves_execution_errors() {
            let service = Service::new(|_: i32| Err::<(), _>("execution failed"));
            service.start().unwrap();
            assert_eq!(
                service.invoke_checked(1, |_: &i32| Ok::<(), &'static str>(())),
                Err(CheckedServiceError::Execution("execution failed"))
            );
        }

        #[test]
        fn try_checked_rejects_invalid_input_and_preserves_execution_errors() {
            let service = Service::new(|value: i32| {
                if value == 7 {
                    Err("execution failed")
                } else {
                    Ok(value * 2)
                }
            });
            let validate = |value: &i32| {
                if *value >= 0 {
                    Ok(())
                } else {
                    Err("negative")
                }
            };
            assert_eq!(
                service.try_invoke_checked(1, validate),
                Err(CheckedServiceError::Lifecycle(ServiceError::NotRunning))
            );
            service.start().unwrap();
            assert_eq!(
                service.try_invoke_checked(-1, validate),
                Err(CheckedServiceError::Validation("negative"))
            );
            assert_eq!(service.try_invoke_checked(3, validate), Ok(6));
            assert_eq!(
                service.try_invoke_checked(7, validate),
                Err(CheckedServiceError::Execution("execution failed"))
            );
            service.dispose().unwrap();
            assert_eq!(
                service.try_invoke_checked(1, validate),
                Err(CheckedServiceError::Lifecycle(ServiceError::Disposed))
            );
        }

        #[test]
        fn try_checked_contention_does_not_run_validation() {
            use std::sync::atomic::{AtomicUsize, Ordering};
            use std::sync::mpsc;
            let (entered_tx, entered_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let service = Arc::new(Service::new(move |value: i32| {
                if value == 1 {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                }
                Ok::<_, ()>(value)
            }));
            service.start().unwrap();
            let running = Arc::clone(&service);
            let worker = thread::spawn(move || running.invoke(1));
            entered_rx.recv().unwrap();
            let validation_calls = AtomicUsize::new(0);
            assert_eq!(
                service.try_invoke_checked(2, |_: &i32| {
                    validation_calls.fetch_add(1, Ordering::SeqCst);
                    Ok::<(), ()>(())
                }),
                Err(CheckedServiceError::Lifecycle(ServiceError::Busy))
            );
            assert_eq!(validation_calls.load(Ordering::SeqCst), 0);
            release_tx.send(()).unwrap();
            assert_eq!(worker.join().unwrap(), Ok(1));
            assert_eq!(
                service.try_invoke_checked(2, |_: &i32| {
                    validation_calls.fetch_add(1, Ordering::SeqCst);
                    Ok::<(), ()>(())
                }),
                Ok(2)
            );
            assert_eq!(validation_calls.load(Ordering::SeqCst), 1);
        }
    }
}
#[cfg(feature = "std")]
pub use std_api::*;
