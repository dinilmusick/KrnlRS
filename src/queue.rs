//! Portable per-service execution queue with bounded FIFO/priority admission.
//! Timeouts and cancellation are cooperative: callbacks cannot be forcibly preempted.
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub cancellation: CancellationToken,
    deadline: Option<Instant>,
}

impl ExecutionContext {
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    pub fn is_timed_out(&self) -> bool {
        self.deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
    }

    pub fn check(&self) -> Result<(), ExecutionControlError> {
        if self.is_cancelled() {
            Err(ExecutionControlError::Cancelled)
        } else if self.is_timed_out() {
            Err(ExecutionControlError::TimedOut)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionControlError {
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueError<E> {
    InvalidConfig,
    Full,
    Closed,
    Poisoned,
    Cancelled,
    TimedOut,
    Execution(E),
}

#[derive(Debug, Clone)]
pub struct QueueOptions {
    pub concurrency: usize,
    pub max_waiting: usize,
}

impl Default for QueueOptions {
    fn default() -> Self {
        Self {
            concurrency: 10,
            max_waiting: 1000,
        }
    }
}

#[derive(Debug, Clone)]
pub struct JobOptions {
    pub priority: i32,
    pub retries: usize,
    pub backoff: Duration,
    pub timeout: Option<Duration>,
    pub cancellation: CancellationToken,
}

impl Default for JobOptions {
    fn default() -> Self {
        Self {
            priority: 0,
            retries: 0,
            backoff: Duration::ZERO,
            timeout: None,
            cancellation: CancellationToken::default(),
        }
    }
}

#[derive(Debug)]
struct Waiting {
    id: u64,
    priority: i32,
}

#[derive(Default)]
struct QueueState {
    active: usize,
    waiting: Vec<Waiting>,
    next_id: u64,
    closed: bool,
}

struct Shared {
    state: Mutex<QueueState>,
    cv: Condvar,
    concurrency: usize,
    max_waiting: usize,
}

#[derive(Clone)]
pub struct KrnlQueue {
    shared: Arc<Shared>,
}

struct Permit {
    shared: Arc<Shared>,
}

impl Drop for Permit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.active -= 1;
            self.shared.cv.notify_all();
        }
    }
}

impl KrnlQueue {
    pub fn new(options: QueueOptions) -> Result<Self, QueueError<()>> {
        if options.concurrency == 0 || options.max_waiting == 0 {
            return Err(QueueError::InvalidConfig);
        }
        Ok(Self {
            shared: Arc::new(Shared {
                state: Mutex::new(QueueState::default()),
                cv: Condvar::new(),
                concurrency: options.concurrency,
                max_waiting: options.max_waiting,
            }),
        })
    }

    pub fn close(&self) -> Result<(), QueueError<()>> {
        let mut state = self.shared.state.lock().map_err(|_| QueueError::Poisoned)?;
        state.closed = true;
        self.shared.cv.notify_all();
        Ok(())
    }

    pub fn counts(&self) -> Result<(usize, usize), QueueError<()>> {
        let state = self.shared.state.lock().map_err(|_| QueueError::Poisoned)?;
        Ok((state.active, state.waiting.len()))
    }

    fn admission<E>(
        &self,
        context: &ExecutionContext,
        priority: i32,
    ) -> Result<Permit, QueueError<E>> {
        let mut state = self.shared.state.lock().map_err(|_| QueueError::Poisoned)?;
        if state.closed {
            return Err(QueueError::Closed);
        }
        if state.waiting.len() >= self.shared.max_waiting {
            return Err(QueueError::Full);
        }
        let id = state.next_id;
        state.next_id = state.next_id.wrapping_add(1);
        state.waiting.push(Waiting { id, priority });
        state
            .waiting
            .sort_by(|a, b| b.priority.cmp(&a.priority).then(a.id.cmp(&b.id)));
        loop {
            let result = if state.closed {
                Some(QueueError::Closed)
            } else {
                match context.check() {
                    Err(ExecutionControlError::Cancelled) => Some(QueueError::Cancelled),
                    Err(ExecutionControlError::TimedOut) => Some(QueueError::TimedOut),
                    Ok(()) => None,
                }
            };
            if let Some(error) = result {
                state.waiting.retain(|waiting| waiting.id != id);
                self.shared.cv.notify_all();
                return Err(error);
            }
            if state
                .waiting
                .first()
                .is_some_and(|waiting| waiting.id == id)
                && state.active < self.shared.concurrency
            {
                state.waiting.remove(0);
                state.active += 1;
                self.shared.cv.notify_all();
                return Ok(Permit {
                    shared: self.shared.clone(),
                });
            }
            let (next, _) = self
                .shared
                .cv
                .wait_timeout(state, Duration::from_millis(10))
                .map_err(|_| QueueError::Poisoned)?;
            state = next;
        }
    }

    /// Runs a job under concurrency admission. Rejected/expired jobs never execute.
    /// Error retries have no implicit side-effect rollback; only retry idempotent jobs.
    pub fn execute<T, E, F>(&self, options: JobOptions, mut job: F) -> Result<T, QueueError<E>>
    where
        F: FnMut(&ExecutionContext) -> Result<T, E>,
    {
        let deadline = options
            .timeout
            .and_then(|duration| Instant::now().checked_add(duration));
        let context = ExecutionContext {
            cancellation: options.cancellation,
            deadline,
        };
        let _permit = self.admission(&context, options.priority)?;
        let mut attempt = 0usize;
        loop {
            match context.check() {
                Ok(()) => {}
                Err(ExecutionControlError::Cancelled) => return Err(QueueError::Cancelled),
                Err(ExecutionControlError::TimedOut) => return Err(QueueError::TimedOut),
            }
            let value = job(&context);
            match context.check() {
                Ok(()) => {}
                Err(ExecutionControlError::Cancelled) => return Err(QueueError::Cancelled),
                Err(ExecutionControlError::TimedOut) => return Err(QueueError::TimedOut),
            }
            match value {
                Ok(value) => return Ok(value),
                Err(error) => {
                    if attempt >= options.retries {
                        return Err(QueueError::Execution(error));
                    }
                }
            }
            attempt += 1;
            if !options.backoff.is_zero() {
                let multiplier = 1u32 << attempt.saturating_sub(1).min(10);
                let sleep_time = options.backoff.saturating_mul(multiplier);
                std::thread::sleep(sleep_time);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    #[test]
    fn retries_and_execution_errors_are_explicit() {
        let queue = KrnlQueue::new(QueueOptions::default()).unwrap();
        let mut attempts = 0;
        let result: Result<i32, QueueError<&'static str>> = queue.execute(
            JobOptions {
                retries: 2,
                ..Default::default()
            },
            |_| {
                attempts += 1;
                if attempts < 3 {
                    Err("retry")
                } else {
                    Ok(42)
                }
            },
        );
        assert_eq!(result, Ok(42));
        assert_eq!(attempts, 3);
        assert_eq!(
            queue.execute(JobOptions::default(), |_| -> Result<(), &str> {
                Err("failed")
            }),
            Err(QueueError::Execution("failed"))
        );
    }

    #[test]
    fn cancellation_and_expired_callbacks_fail_closed() {
        let queue = KrnlQueue::new(QueueOptions::default()).unwrap();
        let token = CancellationToken::default();
        token.cancel();
        let result: Result<(), QueueError<()>> = queue.execute(
            JobOptions {
                cancellation: token,
                ..Default::default()
            },
            |_| panic!("cancelled callback must never run"),
        );
        assert_eq!(result, Err(QueueError::Cancelled));
        let result: Result<(), QueueError<()>> = queue.execute(
            JobOptions {
                timeout: Some(Duration::from_millis(1)),
                ..Default::default()
            },
            |_| {
                thread::sleep(Duration::from_millis(5));
                Ok(())
            },
        );
        assert_eq!(result, Err(QueueError::TimedOut));
    }

    #[test]
    fn bounded_concurrency_and_shutdown() {
        let queue = Arc::new(
            KrnlQueue::new(QueueOptions {
                concurrency: 2,
                max_waiting: 16,
            })
            .unwrap(),
        );
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let (queue, active, peak) = (queue.clone(), active.clone(), peak.clone());
            handles.push(thread::spawn(move || {
                queue.execute(JobOptions::default(), |_| -> Result<(), ()> {
                    let n = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(n, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(6));
                    active.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                })
            }));
        }
        for handle in handles {
            assert_eq!(handle.join().unwrap(), Ok(()));
        }
        assert!(peak.load(Ordering::SeqCst) <= 2);
        assert_eq!(queue.counts(), Ok((0, 0)));
        queue.close().unwrap();
        assert_eq!(
            queue.execute(JobOptions::default(), |_| Ok::<_, ()>(1)),
            Err(QueueError::Closed)
        );
    }
}
