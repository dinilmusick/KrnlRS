//! Deterministic service lifecycle aligned with the KrnlTS core contract.
//! The lifecycle is deliberately transport- and platform-independent.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LifecycleState {
    Uninitialized,
    Initializing,
    Ready,
    Starting,
    Running,
    Stopping,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifecycleError {
    pub from: LifecycleState,
    pub to: LifecycleState,
}

impl std::fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid service lifecycle transition: {:?} -> {:?}",
            self.from, self.to
        )
    }
}

impl std::error::Error for LifecycleError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lifecycle {
    state: LifecycleState,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl Lifecycle {
    pub const fn new() -> Self {
        Self {
            state: LifecycleState::Uninitialized,
        }
    }

    pub const fn state(&self) -> LifecycleState {
        self.state
    }

    pub fn transition(&mut self, next: LifecycleState) -> Result<(), LifecycleError> {
        use LifecycleState::*;
        let allowed = matches!(
            (self.state, next),
            (Uninitialized, Initializing)
                | (Initializing, Ready)
                | (Initializing, Uninitialized) // failed onInit restores uninitialized
                | (Ready, Starting)
                | (Starting, Running)
                | (Starting, Ready) // failed onStart restores ready
                | (Running, Stopping)
                | (Stopping, Stopped)
                | (Stopping, Running) // failed onStop restores running
                | (Stopped, Starting)
        );
        if !allowed {
            return Err(LifecycleError {
                from: self.state,
                to: next,
            });
        }
        self.state = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_start_stop_restart() {
        let mut lifecycle = Lifecycle::new();
        for state in [
            LifecycleState::Initializing,
            LifecycleState::Ready,
            LifecycleState::Starting,
            LifecycleState::Running,
            LifecycleState::Stopping,
            LifecycleState::Stopped,
            LifecycleState::Starting,
            LifecycleState::Running,
        ] {
            lifecycle.transition(state).unwrap();
            assert_eq!(lifecycle.state(), state);
        }
    }

    #[test]
    fn invalid_transition_preserves_state() {
        let mut lifecycle = Lifecycle::new();
        let error = lifecycle.transition(LifecycleState::Running).unwrap_err();
        assert_eq!(error.from, LifecycleState::Uninitialized);
        assert_eq!(error.to, LifecycleState::Running);
        assert_eq!(lifecycle.state(), LifecycleState::Uninitialized);
        lifecycle.transition(LifecycleState::Initializing).unwrap();
        assert!(lifecycle.transition(LifecycleState::Initializing).is_err());
        assert_eq!(lifecycle.state(), LifecycleState::Initializing);
    }

    #[test]
    fn exhaustive_canonical_transition_matrix_preserves_state_on_rejection() {
        use LifecycleState::*;
        // Canonical KrnlTS packages/core/src/lifecycle/LifecycleManager.ts,
        // including three error-handler rollback paths.
        let states = [
            Uninitialized,
            Initializing,
            Ready,
            Starting,
            Running,
            Stopping,
            Stopped,
        ];
        for from in states {
            for to in states {
                let mut lifecycle = Lifecycle { state: from };
                let valid = matches!(
                    (from, to),
                    (Uninitialized, Initializing)
                        | (Initializing, Ready)
                        | (Initializing, Uninitialized)
                        | (Ready, Starting)
                        | (Starting, Running)
                        | (Starting, Ready)
                        | (Running, Stopping)
                        | (Stopping, Stopped)
                        | (Stopping, Running)
                        | (Stopped, Starting)
                );
                let result = lifecycle.transition(to);
                assert_eq!(result.is_ok(), valid, "{from:?} -> {to:?}");
                assert_eq!(lifecycle.state(), if valid { to } else { from });
                if !valid {
                    assert_eq!(result.unwrap_err(), LifecycleError { from, to });
                }
            }
        }
    }

    #[test]
    fn failed_hooks_rollback_and_permit_retry() {
        use LifecycleState::*;
        let mut lifecycle = Lifecycle::new();
        // onInit rejection: initializing -> uninitialized, then retry.
        lifecycle.transition(Initializing).unwrap();
        lifecycle.transition(Uninitialized).unwrap();
        lifecycle.transition(Initializing).unwrap();
        lifecycle.transition(Ready).unwrap();
        // onStart rejection: starting -> ready, then retry.
        lifecycle.transition(Starting).unwrap();
        lifecycle.transition(Ready).unwrap();
        lifecycle.transition(Starting).unwrap();
        lifecycle.transition(Running).unwrap();
        // onStop rejection: stopping -> running, then retry.
        lifecycle.transition(Stopping).unwrap();
        lifecycle.transition(Running).unwrap();
        lifecycle.transition(Stopping).unwrap();
        lifecycle.transition(Stopped).unwrap();
        lifecycle.transition(Starting).unwrap();
        lifecycle.transition(Running).unwrap();
        assert_eq!(lifecycle.state(), Running);
    }
}
