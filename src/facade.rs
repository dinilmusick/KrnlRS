//! Unified service construction, lifecycle, bounded execution and telemetry.
//! All functionality belongs to one service. Cross-service composition is Skipo-owned.
use crate::lifecycle::{Lifecycle, LifecycleError, LifecycleState};
use crate::queue::{ExecutionContext, JobOptions, KrnlQueue, QueueError, QueueOptions};
use crate::registry::{RegistryError, ServiceDescriptor, ServiceRegistry};
use crate::telemetry::{InvocationTrace, Telemetry, TelemetryMode, TelemetrySnapshot};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::Instant;

pub trait LifecycleHooks: Send {
    type Error;
    fn on_initialize(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn on_start(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn on_stop(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn on_dispose(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Default)]
pub struct NoopHooks;
impl LifecycleHooks for NoopHooks {
    type Error = ();
}

#[derive(Debug, PartialEq, Eq)]
pub enum FacadeError<E> {
    InvalidDefinition(RegistryError),
    InvalidQueue,
    Lifecycle(LifecycleError),
    NotRunning(LifecycleState),
    Disposed,
    Hook(E),
    Poisoned,
}

#[derive(Debug, PartialEq, Eq)]
pub enum InvocationFailure<V, E> {
    Input(V),
    Execution(E),
    Output(V),
}

#[derive(Debug, PartialEq, Eq)]
pub enum InvokeError<H, V, E> {
    Lifecycle(FacadeError<H>),
    Queue(QueueError<InvocationFailure<V, E>>),
    TelemetryPoisoned,
}

struct Phase {
    lifecycle: Lifecycle,
    disposed: bool,
    active: usize,
}

pub struct ServiceFacade<H: LifecycleHooks> {
    pub definition: ServiceDescriptor,
    hooks: Mutex<H>,
    phase: Mutex<Phase>,
    drain: Condvar,
    queue: KrnlQueue,
    telemetry: Mutex<Telemetry>,
}

struct RunningInvocation<'a, H: LifecycleHooks>(&'a ServiceFacade<H>);
impl<H: LifecycleHooks> Drop for RunningInvocation<'_, H> {
    fn drop(&mut self) {
        if let Ok(mut phase) = self.0.phase.lock() {
            phase.active -= 1;
            self.0.drain.notify_all();
        }
    }
}

impl<H: LifecycleHooks> ServiceFacade<H> {
    pub fn new(
        definition: ServiceDescriptor,
        hooks: H,
        queue_options: QueueOptions,
        telemetry_capacity: usize,
    ) -> Result<Self, FacadeError<H::Error>> {
        let mut registry = ServiceRegistry::new();
        registry
            .register(definition.clone())
            .map_err(FacadeError::InvalidDefinition)?;
        let queue = KrnlQueue::new(queue_options).map_err(|_| FacadeError::InvalidQueue)?;
        Ok(Self {
            definition,
            hooks: Mutex::new(hooks),
            phase: Mutex::new(Phase {
                lifecycle: Lifecycle::new(),
                disposed: false,
                active: 0,
            }),
            drain: Condvar::new(),
            queue,
            telemetry: Mutex::new(Telemetry::new(telemetry_capacity)),
        })
    }

    fn lock_phase(&self) -> Result<MutexGuard<'_, Phase>, FacadeError<H::Error>> {
        self.phase.lock().map_err(|_| FacadeError::Poisoned)
    }

    pub fn status(&self) -> Result<(LifecycleState, bool), FacadeError<H::Error>> {
        let phase = self.lock_phase()?;
        Ok((phase.lifecycle.state(), phase.disposed))
    }

    pub fn initialize(&self) -> Result<(), FacadeError<H::Error>> {
        let mut phase = self.lock_phase()?;
        if phase.disposed {
            return Err(FacadeError::Disposed);
        }
        if phase.lifecycle.state() != LifecycleState::Uninitialized {
            return Err(FacadeError::NotRunning(phase.lifecycle.state()));
        }
        phase
            .lifecycle
            .transition(LifecycleState::Initializing)
            .map_err(FacadeError::Lifecycle)?;
        let action = self
            .hooks
            .lock()
            .map_err(|_| FacadeError::Poisoned)?
            .on_initialize();
        match action {
            Ok(()) => phase
                .lifecycle
                .transition(LifecycleState::Ready)
                .map_err(FacadeError::Lifecycle),
            Err(error) => {
                phase
                    .lifecycle
                    .transition(LifecycleState::Uninitialized)
                    .map_err(FacadeError::Lifecycle)?;
                Err(FacadeError::Hook(error))
            }
        }
    }

    pub fn start(&self) -> Result<(), FacadeError<H::Error>> {
        {
            let phase = self.lock_phase()?;
            if phase.disposed {
                return Err(FacadeError::Disposed);
            }
            if phase.lifecycle.state() == LifecycleState::Running {
                return Ok(());
            }
        }
        if self.status()?.0 == LifecycleState::Uninitialized {
            self.initialize()?;
        }
        let mut phase = self.lock_phase()?;
        if phase.disposed {
            return Err(FacadeError::Disposed);
        }
        match phase.lifecycle.state() {
            LifecycleState::Ready | LifecycleState::Stopped => {}
            state => return Err(FacadeError::NotRunning(state)),
        }
        phase
            .lifecycle
            .transition(LifecycleState::Starting)
            .map_err(FacadeError::Lifecycle)?;
        let action = self
            .hooks
            .lock()
            .map_err(|_| FacadeError::Poisoned)?
            .on_start();
        match action {
            Ok(()) => phase
                .lifecycle
                .transition(LifecycleState::Running)
                .map_err(FacadeError::Lifecycle),
            Err(error) => {
                phase
                    .lifecycle
                    .transition(LifecycleState::Ready)
                    .map_err(FacadeError::Lifecycle)?;
                Err(FacadeError::Hook(error))
            }
        }
    }

    pub fn stop(&self) -> Result<(), FacadeError<H::Error>> {
        let mut phase = self.lock_phase()?;
        if phase.disposed {
            return Err(FacadeError::Disposed);
        }
        if phase.lifecycle.state() == LifecycleState::Stopped {
            return Ok(());
        }
        if phase.lifecycle.state() != LifecycleState::Running {
            return Err(FacadeError::NotRunning(phase.lifecycle.state()));
        }
        phase
            .lifecycle
            .transition(LifecycleState::Stopping)
            .map_err(FacadeError::Lifecycle)?;
        while phase.active != 0 {
            phase = self.drain.wait(phase).map_err(|_| FacadeError::Poisoned)?;
        }
        let action = self
            .hooks
            .lock()
            .map_err(|_| FacadeError::Poisoned)?
            .on_stop();
        match action {
            Ok(()) => phase
                .lifecycle
                .transition(LifecycleState::Stopped)
                .map_err(FacadeError::Lifecycle),
            Err(error) => {
                phase
                    .lifecycle
                    .transition(LifecycleState::Running)
                    .map_err(FacadeError::Lifecycle)?;
                Err(FacadeError::Hook(error))
            }
        }
    }

    pub fn restart(&self) -> Result<(), FacadeError<H::Error>> {
        if self.status()?.0 == LifecycleState::Running {
            self.stop()?;
        }
        self.start()
    }

    pub fn dispose(&self) -> Result<(), FacadeError<H::Error>> {
        if self.status()?.1 {
            return Ok(());
        }
        if self.status()?.0 == LifecycleState::Running {
            self.stop()?;
        }
        let mut phase = self.lock_phase()?;
        while phase.active != 0 {
            phase = self.drain.wait(phase).map_err(|_| FacadeError::Poisoned)?;
        }
        self.hooks
            .lock()
            .map_err(|_| FacadeError::Poisoned)?
            .on_dispose()
            .map_err(FacadeError::Hook)?;
        phase.disposed = true;
        self.queue.close().map_err(|_| FacadeError::InvalidQueue)
    }

    fn enter(&self) -> Result<RunningInvocation<'_, H>, FacadeError<H::Error>> {
        let mut phase = self.lock_phase()?;
        if phase.disposed {
            return Err(FacadeError::Disposed);
        }
        if phase.lifecycle.state() != LifecycleState::Running {
            return Err(FacadeError::NotRunning(phase.lifecycle.state()));
        }
        phase.active += 1;
        Ok(RunningInvocation(self))
    }

    pub fn set_telemetry_mode(&self, mode: TelemetryMode) -> Result<(), FacadeError<H::Error>> {
        self.telemetry
            .lock()
            .map_err(|_| FacadeError::Poisoned)?
            .set_mode(mode);
        Ok(())
    }

    pub fn telemetry_snapshot(&self) -> Result<TelemetrySnapshot, FacadeError<H::Error>> {
        Ok(self
            .telemetry
            .lock()
            .map_err(|_| FacadeError::Poisoned)?
            .snapshot())
    }

    /// Typed invocation with validated inputs/outputs, context, retry, queue and telemetry.
    /// Retry callbacks may execute more than once. Handlers should be idempotent when retries > 0.
    pub fn invoke<I, O, V, E, F, IV, OV>(
        &self,
        name: &str,
        input: I,
        options: JobOptions,
        correlation_id: Option<String>,
        validate_input: IV,
        mut handler: F,
        validate_output: OV,
    ) -> Result<O, InvokeError<H::Error, V, E>>
    where
        I: Clone,
        F: FnMut(I, &ExecutionContext) -> Result<O, E>,
        IV: Fn(&I) -> Result<(), V>,
        OV: Fn(&O) -> Result<(), V>,
    {
        let _running = self.enter().map_err(InvokeError::Lifecycle)?;
        let started = Instant::now();
        let result = self.queue.execute(options, |context| {
            validate_input(&input).map_err(InvocationFailure::Input)?;
            let output = handler(input.clone(), context).map_err(InvocationFailure::Execution)?;
            validate_output(&output).map_err(InvocationFailure::Output)?;
            Ok(output)
        });
        self.telemetry
            .lock()
            .map_err(|_| InvokeError::TelemetryPoisoned)?
            .record(InvocationTrace {
                operation: name.to_owned(),
                correlation_id,
                elapsed: started.elapsed(),
                succeeded: result.is_ok(),
            });
        result.map_err(InvokeError::Queue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    fn facade() -> ServiceFacade<NoopHooks> {
        ServiceFacade::new(
            ServiceDescriptor::new("clock", "1"),
            NoopHooks,
            QueueOptions::default(),
            2,
        )
        .unwrap()
    }

    #[test]
    fn validates_invocation_and_records_bounded_correlated_telemetry() {
        let facade = facade();
        assert_eq!(facade.status(), Ok((LifecycleState::Uninitialized, false)));
        facade.start().unwrap();
        facade.set_telemetry_mode(TelemetryMode::Detailed).unwrap();
        let out = facade.invoke(
            "tick",
            5i32,
            JobOptions::default(),
            Some("corr-a".into()),
            |value| if *value >= 0 { Ok(()) } else { Err("negative") },
            |value, _| Ok::<_, &'static str>(value + 1),
            |value| if *value <= 100 { Ok(()) } else { Err("large") },
        );
        assert_eq!(out, Ok(6));
        assert_eq!(
            facade.telemetry_snapshot().unwrap().operations["tick"].successes,
            1
        );
        assert_eq!(
            facade.telemetry_snapshot().unwrap().recent[0]
                .correlation_id
                .as_deref(),
            Some("corr-a")
        );
        facade.stop().unwrap();
        let denied = facade.invoke(
            "tick",
            1,
            JobOptions::default(),
            None,
            |_| Ok::<_, &'static str>(()),
            |value, _| Ok::<_, &'static str>(value),
            |_| Ok::<_, &'static str>(()),
        );
        assert_eq!(
            denied,
            Err(InvokeError::Lifecycle(FacadeError::NotRunning(
                LifecycleState::Stopped
            )))
        );
        facade.restart().unwrap();
        facade.dispose().unwrap();
        assert_eq!(facade.start(), Err(FacadeError::Disposed));
        assert!(facade.dispose().is_ok());
    }

    struct RejectStart;
    impl LifecycleHooks for RejectStart {
        type Error = &'static str;
        fn on_start(&mut self) -> Result<(), Self::Error> {
            Err("rejected")
        }
    }

    #[test]
    fn failed_start_restores_ready_and_cannot_invoke() {
        let facade = ServiceFacade::new(
            ServiceDescriptor::new("broken", "1"),
            RejectStart,
            QueueOptions::default(),
            1,
        )
        .unwrap();
        assert_eq!(facade.start(), Err(FacadeError::Hook("rejected")));
        assert_eq!(facade.status(), Ok((LifecycleState::Ready, false)));
        assert_eq!(facade.start(), Err(FacadeError::Hook("rejected")));
    }

    #[test]
    fn stop_waits_for_in_flight_handler_then_disposes() {
        let facade = Arc::new(facade());
        facade.start().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = facade.clone();
        let join = thread::spawn(move || {
            worker.invoke(
                "slow",
                1,
                JobOptions::default(),
                None,
                |_| Ok::<_, ()>(()),
                |value, _| {
                    tx.send(()).unwrap();
                    thread::sleep(Duration::from_millis(20));
                    Ok::<_, ()>(value + 1)
                },
                |_| Ok::<_, ()>(()),
            )
        });
        rx.recv().unwrap();
        facade.stop().unwrap();
        assert_eq!(join.join().unwrap(), Ok(2));
        assert_eq!(facade.status(), Ok((LifecycleState::Stopped, false)));
        facade.dispose().unwrap();
        assert_eq!(facade.status(), Ok((LifecycleState::Stopped, true)));
    }
}
