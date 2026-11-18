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
        write!(f, "invalid service lifecycle transition: {:?} -> {:?}", self.from, self.to)
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
        Self { state: LifecycleState::Uninitialized }
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
                | (Ready, Starting)
                | (Starting, Running)
                | (Running, Stopping)
                | (Stopping, Stopped)
                | (Stopped, Starting)
        );
        if !allowed {
            return Err(LifecycleError { from: self.state, to: next });
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
    fn all_unsupported_transitions_are_rejected() {
        use LifecycleState::*;
        let states = [Uninitialized, Initializing, Ready, Starting, Running, Stopping, Stopped];
        for from in states {
            for to in states {
                let mut lifecycle = Lifecycle { state: from };
                let valid = matches!((from, to),
                    (Uninitialized, Initializing) | (Initializing, Ready)
                    | (Ready, Starting) | (Starting, Running)
                    | (Running, Stopping) | (Stopping, Stopped)
                    | (Stopped, Starting));
                assert_eq!(lifecycle.transition(to).is_ok(), valid, "{from:?} -> {to:?}");
                assert_eq!(lifecycle.state(), if valid { to } else { from });
            }
        }
    }
}
