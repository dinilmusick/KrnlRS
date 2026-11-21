//! Alloc-compatible, host-neutral foundations for embedded or bare-metal consumers.
use alloc::string::{String, ToString};
use alloc::vec::Vec;

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

impl LifecycleState {
    pub const fn permits(self, next: Self) -> bool {
        use LifecycleState::*;
        matches!(
            (self, next),
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
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidTransition {
    pub from: LifecycleState,
    pub to: LifecycleState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortableLifecycle {
    state: LifecycleState,
}

impl Default for PortableLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl PortableLifecycle {
    pub const fn new() -> Self {
        Self {
            state: LifecycleState::Uninitialized,
        }
    }

    pub const fn state(&self) -> LifecycleState {
        self.state
    }

    pub fn transition(&mut self, next: LifecycleState) -> Result<(), InvalidTransition> {
        if !self.state.permits(next) {
            return Err(InvalidTransition {
                from: self.state,
                to: next,
            });
        }
        self.state = next;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Identifier(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentifierError {
    Empty,
    InvalidCharacter,
}

impl Identifier {
    pub fn parse(raw: &str) -> Result<Self, IdentifierError> {
        if raw.trim().is_empty() {
            return Err(IdentifierError::Empty);
        }
        if !raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        {
            return Err(IdentifierError::InvalidCharacter);
        }
        Ok(Self(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationContract {
    pub name: Identifier,
    pub inputs: Vec<Identifier>,
    pub output: Identifier,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_contracts_reject_unusable_identifiers() {
        assert_eq!(Identifier::parse(" "), Err(IdentifierError::Empty));
        assert_eq!(
            Identifier::parse("http://"),
            Err(IdentifierError::InvalidCharacter)
        );
        assert_eq!(
            Identifier::parse("storage.read").unwrap().as_str(),
            "storage.read"
        );
    }

    #[test]
    fn alloc_lifecycle_rejects_invalid_transition_without_mutating() {
        let mut lifecycle = PortableLifecycle::new();
        assert!(lifecycle.transition(LifecycleState::Running).is_err());
        assert_eq!(lifecycle.state(), LifecycleState::Uninitialized);
        lifecycle.transition(LifecycleState::Initializing).unwrap();
        lifecycle.transition(LifecycleState::Ready).unwrap();
    }
}
