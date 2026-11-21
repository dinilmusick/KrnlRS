//! Portable service-manifest validation with actionable source-line diagnostics.
use crate::registry::{RegistryError, ServiceDescriptor, ServiceRegistry};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub line: usize,
    pub code: &'static str,
    pub message: String,
}

pub fn validate_manifest(text: &str) -> Result<ServiceRegistry, Vec<Diagnostic>> {
    let mut definitions = BTreeMap::<String, (ServiceDescriptor, usize)>::new();
    let mut instructions = Vec::<(usize, String, String, String)>::new();
    let mut errors = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let columns: Vec<&str> = trimmed.split_whitespace().collect();
        match columns.as_slice() {
            ["service", id, version] => {
                if definitions.contains_key(*id) {
                    errors.push(Diagnostic {
                        line,
                        code: "duplicate_service",
                        message: format!("service {id:?} is defined more than once"),
                    });
                } else {
                    definitions.insert((*id).into(), (ServiceDescriptor::new(*id, *version), line));
                }
            }
            ["capability" | "depends", id, target] => {
                instructions.push((line, columns[0].into(), (*id).into(), (*target).into()));
            }
            _ => errors.push(Diagnostic {
                line,
                code: "invalid_syntax",
                message: "expected 'service ID VERSION', 'capability ID NAME', or 'depends ID ID'"
                    .into(),
            }),
        }
    }
    for (line, command, id, target) in instructions {
        if let Some((descriptor, _)) = definitions.get_mut(&id) {
            if command == "depends" {
                descriptor.dependencies.insert(target);
            } else {
                descriptor.capabilities.insert(target);
            }
        } else {
            errors.push(Diagnostic {
                line,
                code: "unknown_service",
                message: format!("service {id:?} is not declared"),
            });
        }
    }
    let mut registry = ServiceRegistry::new();
    for (_, (descriptor, line)) in definitions {
        if let Err(error) = registry.register(descriptor) {
            errors.push(Diagnostic {
                line,
                code: "invalid_definition",
                message: format!("{error:?}"),
            });
        }
    }
    for descriptor in registry.list() {
        if let Err(error) = registry.resolve_order(&descriptor.id) {
            let code = match error {
                RegistryError::MissingService(_) => "missing_dependency",
                RegistryError::DependencyCycle(_) => "dependency_cycle",
                _ => "invalid_dependency",
            };
            if !errors.iter().any(|diagnostic| diagnostic.code == code) {
                errors.push(Diagnostic {
                    line: 0,
                    code,
                    message: format!("{error:?}"),
                });
            }
        }
    }
    errors.sort_by_key(|error| (error.line, error.code));
    if errors.is_empty() {
        Ok(registry)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_manifest_and_reports_line_diagnostics() {
        let valid = "service api 1.0\nservice db 1\ndepends api db\ncapability db read\n";
        let registry = validate_manifest(valid).unwrap();
        assert_eq!(registry.resolve_order("api").unwrap(), vec!["db", "api"]);
        assert_eq!(registry.with_capability("read").len(), 1);
        let invalid = "service app 1\nservice app 2\ndepends app missing\nunknown op\n";
        let errors = validate_manifest(invalid).unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.line == 2 && e.code == "duplicate_service"));
        assert!(errors
            .iter()
            .any(|e| e.line == 4 && e.code == "invalid_syntax"));
        assert!(errors.iter().any(|e| e.code == "missing_dependency"));
    }
}
