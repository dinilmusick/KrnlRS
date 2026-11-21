//! Typed in-process service events; subscribers are service-scoped.
use std::collections::BTreeMap;
use std::sync::{mpsc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventError {
    Poisoned,
    EmptyTopic,
}

pub struct ServiceEvents<E> {
    subscribers: Mutex<BTreeMap<String, Vec<mpsc::Sender<E>>>>,
}

impl<E: Clone> Default for ServiceEvents<E> {
    fn default() -> Self {
        Self {
            subscribers: Mutex::new(BTreeMap::new()),
        }
    }
}

impl<E: Clone> ServiceEvents<E> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscribe(&self, topic: &str) -> Result<mpsc::Receiver<E>, EventError> {
        if topic.trim().is_empty() {
            return Err(EventError::EmptyTopic);
        }
        let (tx, rx) = mpsc::channel();
        self.subscribers
            .lock()
            .map_err(|_| EventError::Poisoned)?
            .entry(topic.to_owned())
            .or_default()
            .push(tx);
        Ok(rx)
    }

    pub fn publish(&self, topic: &str, event: E) -> Result<usize, EventError> {
        if topic.trim().is_empty() {
            return Err(EventError::EmptyTopic);
        }
        let mut guard = self.subscribers.lock().map_err(|_| EventError::Poisoned)?;
        let mut delivered = 0;
        if let Some(receivers) = guard.get_mut(topic) {
            receivers.retain(|tx| {
                if tx.send(event.clone()).is_ok() {
                    delivered += 1;
                    true
                } else {
                    false
                }
            });
        }
        Ok(delivered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatches_only_to_topic_and_prunes_disconnected_subscribers() {
        let bus = ServiceEvents::new();
        let rx = bus.subscribe("changed").unwrap();
        let rx_other = bus.subscribe("other").unwrap();
        assert_eq!(bus.publish("changed", 12).unwrap(), 1);
        assert_eq!(rx.try_recv().unwrap(), 12);
        assert!(rx_other.try_recv().is_err());
        drop(rx);
        assert_eq!(bus.publish("changed", 13).unwrap(), 0);
        assert_eq!(bus.subscribe(" "), Err(EventError::EmptyTopic));
    }
}
