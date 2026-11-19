//! Deterministic, single-service functionality orchestration.
//! Cross-service dispatch belongs to Skipo, not this module.
use std::collections::{BTreeMap, BTreeSet};

pub type NodeAction<O, E> =
    Box<dyn Fn(&BTreeMap<String, O>) -> Result<O, E> + Send + Sync>;

pub struct FunctionNode<O, E> {
    pub id: String,
    pub dependencies: Vec<String>,
    pub execute: NodeAction<O, E>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum OrchestrationError<E> {
    DuplicateNode(String),
    MissingDependency { node: String, dependency: String },
    Cycle,
    Execution { node: String, error: E },
}

/// A dependency DAG whose nodes all belong to one service.
/// Inputs to a node contain only its declared predecessors, not unrelated outputs.
pub struct FunctionalityOrchestration<O, E> {
    nodes: BTreeMap<String, FunctionNode<O, E>>,
}

impl<O, E> Default for FunctionalityOrchestration<O, E> {
    fn default() -> Self {
        Self { nodes: BTreeMap::new() }
    }
}

impl<O, E> FunctionalityOrchestration<O, E> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, node: FunctionNode<O, E>) -> Result<(), OrchestrationError<E>> {
        if self.nodes.contains_key(&node.id) {
            return Err(OrchestrationError::DuplicateNode(node.id));
        }
        self.nodes.insert(node.id.clone(), node);
        Ok(())
    }

    /// Validates all edges and cycles before executing any functionality.
    /// Ready nodes execute in lexical ID order, giving reproducible ordering.
    pub fn run(&self) -> Result<BTreeMap<String, O>, OrchestrationError<E>>
    where
        O: Clone,
    {
        for (id, node) in &self.nodes {
            for dep in &node.dependencies {
                if !self.nodes.contains_key(dep) {
                    return Err(OrchestrationError::MissingDependency {
                        node: id.clone(), dependency: dep.clone(),
                    });
                }
            }
        }
        let mut completed = BTreeSet::new();
        let mut order = Vec::with_capacity(self.nodes.len());
        while order.len() < self.nodes.len() {
            let next = self.nodes.iter().find(|(id, node)| {
                !completed.contains(*id)
                    && node.dependencies.iter().all(|dep| completed.contains(dep))
            });
            match next {
                Some((id, _)) => {
                    completed.insert(id.clone());
                    order.push(id.clone());
                }
                None => return Err(OrchestrationError::Cycle),
            }
        }
        let mut results = BTreeMap::new();
        for id in order {
            let node = &self.nodes[&id];
            let inputs = node.dependencies.iter()
                .map(|dep| (dep.clone(), results[dep].clone()))
                .collect();
            let output = (node.execute)(&inputs).map_err(|error| {
                OrchestrationError::Execution { node: id.clone(), error }
            })?;
            results.insert(id, output);
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn node(id: &str, deps: &[&str], f: impl Fn(&BTreeMap<String, i32>) -> Result<i32, &'static str> + Send + Sync + 'static) -> FunctionNode<i32, &'static str> {
        FunctionNode { id: id.into(), dependencies: deps.iter().map(|s| (*s).into()).collect(), execute: Box::new(f) }
    }

    #[test]
    fn fan_out_and_join_are_deterministic_and_scoped() {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let mut graph = FunctionalityOrchestration::new();
        for (id, deps, value) in [("z", vec!["a"], 3), ("a", vec![], 1), ("b", vec!["a"], 2)] {
            let trace = trace.clone();
            graph.add(node(id, &deps, move |inputs| {
                trace.lock().unwrap().push((id, inputs.keys().cloned().collect::<Vec<_>>()));
                Ok(value)
            })).unwrap();
        }
        graph.add(node("join", &["b", "z"], |inputs| Ok(inputs.values().sum()))).unwrap();
        assert_eq!(graph.run().unwrap()["join"], 5);
        assert_eq!(trace.lock().unwrap().iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec!["a", "b", "z"]);
        assert_eq!(trace.lock().unwrap()[1].1, vec!["a".to_string()]);
    }

    #[test]
    fn missing_edge_and_cycle_fail_before_side_effects() {
        let calls = Arc::new(Mutex::new(0));
        let mut graph = FunctionalityOrchestration::new();
        let seen = calls.clone();
        graph.add(node("a", &[], move |_| { *seen.lock().unwrap() += 1; Ok(1) })).unwrap();
        graph.add(node("b", &["absent"], |_| Ok(2))).unwrap();
        assert_eq!(graph.run(), Err(OrchestrationError::MissingDependency { node: "b".into(), dependency: "absent".into() }));
        assert_eq!(*calls.lock().unwrap(), 0);
        let mut cycle = FunctionalityOrchestration::new();
        cycle.add(node("a", &["b"], |_| Ok(1))).unwrap();
        cycle.add(node("b", &["a"], |_| Ok(2))).unwrap();
        assert_eq!(cycle.run(), Err(OrchestrationError::Cycle));
    }

    #[test]
    fn duplicate_and_execution_failure_are_explicit() {
        let mut graph = FunctionalityOrchestration::new();
        graph.add(node("a", &[], |_| Err("failed"))).unwrap();
        assert!(matches!(graph.add(node("a", &[], |_| Ok(1))), Err(OrchestrationError::DuplicateNode(id)) if id == "a"));
        assert_eq!(graph.run(), Err(OrchestrationError::Execution { node: "a".into(), error: "failed" }));
    }
}
