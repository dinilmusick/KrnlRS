//! Service-owned typed state. No global mutable singleton is introduced.
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateError {
    Poisoned,
}

pub struct ServiceState<S> {
    value: Mutex<S>,
}

impl<S> ServiceState<S> {
    pub fn new(initial: S) -> Self {
        Self {
            value: Mutex::new(initial),
        }
    }

    pub fn read<R>(&self, reader: impl FnOnce(&S) -> R) -> Result<R, StateError> {
        let guard = self.value.lock().map_err(|_| StateError::Poisoned)?;
        Ok(reader(&guard))
    }

    pub fn update<R>(&self, writer: impl FnOnce(&mut S) -> R) -> Result<R, StateError> {
        let mut guard = self.value.lock().map_err(|_| StateError::Poisoned)?;
        Ok(writer(&mut guard))
    }
}

impl<S: Clone> ServiceState<S> {
    pub fn snapshot(&self) -> Result<S, StateError> {
        self.read(Clone::clone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_updates_and_returns_independent_snapshots() {
        let state = ServiceState::new(vec![1u8]);
        let first = state.snapshot().unwrap();
        state.update(|v| v.push(2)).unwrap();
        assert_eq!(first, vec![1]);
        assert_eq!(state.snapshot().unwrap(), vec![1, 2]);
    }
}
