//! Deterministic service registry and dependency/capability discovery.
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceDescriptor {
    pub id: String,
    pub version: String,
    pub capabilities: BTreeSet<String>,
    pub dependencies: BTreeSet<String>,
}

impl ServiceDescriptor {
    pub fn new(id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
            capabilities: BTreeSet::new(),
            dependencies: BTreeSet::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    EmptyIdentifier,
    DuplicateService(String),
    MissingService(String),
    DependencyCycle(String),
    SelfDependency(String),
}

#[derive(Debug, Default, Clone)]
pub struct ServiceRegistry {
    entries: BTreeMap<String, ServiceDescriptor>,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, descriptor: ServiceDescriptor) -> Result<(), RegistryError> {
        if descriptor.id.trim().is_empty()
            || descriptor.version.trim().is_empty()
            || descriptor.capabilities.iter().any(|s| s.trim().is_empty())
            || descriptor.dependencies.iter().any(|s| s.trim().is_empty())
        {
            return Err(RegistryError::EmptyIdentifier);
        }
        if descriptor.dependencies.contains(&descriptor.id) {
            return Err(RegistryError::SelfDependency(descriptor.id));
        }
        if self.entries.contains_key(&descriptor.id) {
            return Err(RegistryError::DuplicateService(descriptor.id));
        }
        self.entries.insert(descriptor.id.clone(), descriptor);
        Ok(())
    }

    pub fn unregister(&mut self, id: &str) -> Option<ServiceDescriptor> {
        self.entries.remove(id)
    }

    pub fn get(&self, id: &str) -> Option<&ServiceDescriptor> {
        self.entries.get(id)
    }

    pub fn list(&self) -> Vec<&ServiceDescriptor> {
        self.entries.values().collect()
    }

    pub fn with_capability(&self, capability: &str) -> Vec<&ServiceDescriptor> {
        self.entries
            .values()
            .filter(|entry| entry.capabilities.contains(capability))
            .collect()
    }

    /// Returns dependency-first ordering, including the requested service.
    /// All unresolved or cyclic dependencies fail deterministically.
    pub fn resolve_order(&self, id: &str) -> Result<Vec<String>, RegistryError> {
        fn visit(
            registry: &ServiceRegistry,
            id: &str,
            visiting: &mut BTreeSet<String>,
            visited: &mut BTreeSet<String>,
            output: &mut Vec<String>,
        ) -> Result<(), RegistryError> {
            if visited.contains(id) {
                return Ok(());
            }
            if !visiting.insert(id.to_owned()) {
                return Err(RegistryError::DependencyCycle(id.to_owned()));
            }
            let record = registry
                .entries
                .get(id)
                .ok_or_else(|| RegistryError::MissingService(id.to_owned()))?;
            for dependency in &record.dependencies {
                visit(registry, dependency, visiting, visited, output)?;
            }
            visiting.remove(id);
            visited.insert(id.to_owned());
            output.push(id.to_owned());
            Ok(())
        }
        let mut output = Vec::new();
        visit(self, id, &mut BTreeSet::new(), &mut BTreeSet::new(), &mut output)?;
        Ok(output)
    }

    pub fn validate_all(&self) -> Result<(), RegistryError> {
        for id in self.entries.keys() {
            self.resolve_order(id)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependencies_are_deterministic_and_capabilities_discoverable() {
        let mut registry = ServiceRegistry::new();
        let mut app = ServiceDescriptor::new("app", "1");
        app.dependencies.insert("storage".into());
        app.dependencies.insert("auth".into());
        registry.register(app).unwrap();
        registry.register(ServiceDescriptor::new("auth", "1")).unwrap();
        let mut storage = ServiceDescriptor::new("storage", "1");
        storage.capabilities.insert("read".into());
        registry.register(storage).unwrap();
        assert_eq!(registry.resolve_order("app").unwrap(), vec!["auth", "storage", "app"]);
        assert_eq!(registry.with_capability("read")[0].id, "storage");
        assert!(registry.validate_all().is_ok());
    }

    #[test]
    fn rejects_duplicates_missing_and_cycles_without_side_effects() {
        let mut registry = ServiceRegistry::new();
        registry.register(ServiceDescriptor::new("one", "1")).unwrap();
        assert_eq!(
            registry.register(ServiceDescriptor::new("one", "2")),
            Err(RegistryError::DuplicateService("one".into()))
        );
        let mut two = ServiceDescriptor::new("two", "1");
        two.dependencies.insert("absent".into());
        registry.register(two).unwrap();
        assert_eq!(
            registry.resolve_order("two"),
            Err(RegistryError::MissingService("absent".into()))
        );
        let mut a = ServiceDescriptor::new("a", "1");
        a.dependencies.insert("b".into());
        let mut b = ServiceDescriptor::new("b", "1");
        b.dependencies.insert("a".into());
        registry.register(a).unwrap();
        registry.register(b).unwrap();
        assert_eq!(registry.resolve_order("a"), Err(RegistryError::DependencyCycle("a".into())));
    }
}
