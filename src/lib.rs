//! Rust-side semantic kernel primitives. Cross-service orchestration belongs to Skipo.

use std::collections::{BTreeMap, BTreeSet};

/// An operation is always owned by exactly one service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Functionality {
    pub service: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    EmptyIdentifier,
    ForeignService { expected: String, actual: String },
    DuplicateFunctionality(String),
    UnknownFunctionality(String),
}

/// A deterministic, single-service composition plan; it does not execute other services.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionalityOrchestration {
    service: String,
    operations: BTreeMap<String, Functionality>,
}

impl FunctionalityOrchestration {
    pub fn new(service: impl Into<String>) -> Result<Self, ContractError> {
        let service = service.into();
        if service.trim().is_empty() { return Err(ContractError::EmptyIdentifier); }
        Ok(Self { service, operations: BTreeMap::new() })
    }

    pub fn register(&mut self, operation: Functionality) -> Result<(), ContractError> {
        if operation.name.trim().is_empty() || operation.service.trim().is_empty() {
            return Err(ContractError::EmptyIdentifier);
        }
        if operation.service != self.service {
            return Err(ContractError::ForeignService {
                expected: self.service.clone(), actual: operation.service,
            });
        }
        if self.operations.contains_key(&operation.name) {
            return Err(ContractError::DuplicateFunctionality(operation.name));
        }
        self.operations.insert(operation.name.clone(), operation);
        Ok(())
    }

    /// Validate a plan before execution. Order and repeated invocations are preserved.
    pub fn validate_plan<'a>(&self, steps: impl IntoIterator<Item = &'a str>) -> Result<Vec<String>, ContractError> {
        let mut validated = Vec::new();
        for step in steps {
            if !self.operations.contains_key(step) {
                return Err(ContractError::UnknownFunctionality(step.to_owned()));
            }
            validated.push(step.to_owned());
        }
        Ok(validated)
    }

    pub fn names(&self) -> BTreeSet<&str> {
        self.operations.keys().map(String::as_str).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_local_operations_and_ordered_repetition() {
        let mut registry = FunctionalityOrchestration::new("clock").unwrap();
        registry.register(Functionality { service: "clock".into(), name: "tick".into() }).unwrap();
        registry.register(Functionality { service: "clock".into(), name: "reset".into() }).unwrap();
        assert_eq!(registry.validate_plan(["tick", "reset", "tick"]).unwrap(), vec!["tick", "reset", "tick"]);
    }

    #[test]
    fn rejects_cross_service_composition() {
        let mut registry = FunctionalityOrchestration::new("clock").unwrap();
        assert_eq!(registry.register(Functionality { service: "storage".into(), name: "read".into() }),
            Err(ContractError::ForeignService { expected: "clock".into(), actual: "storage".into() }));
    }

    #[test]
    fn rejects_duplicates_and_unknown_steps() {
        let mut registry = FunctionalityOrchestration::new("clock").unwrap();
        let op = Functionality { service: "clock".into(), name: "tick".into() };
        registry.register(op.clone()).unwrap();
        assert_eq!(registry.register(op), Err(ContractError::DuplicateFunctionality("tick".into())));
        assert_eq!(registry.validate_plan(["missing"]), Err(ContractError::UnknownFunctionality("missing".into())));
    }

    #[test]
    fn rejects_blank_identifiers() {
        assert_eq!(FunctionalityOrchestration::new("  "), Err(ContractError::EmptyIdentifier));
        let mut registry = FunctionalityOrchestration::new("clock").unwrap();
        assert_eq!(registry.register(Functionality { service: "clock".into(), name: " ".into() }), Err(ContractError::EmptyIdentifier));
    }
}
