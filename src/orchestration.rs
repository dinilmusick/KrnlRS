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
    pub fn execution_order(&self) -> Result<Vec<String>, OrchestrationError<E>> {
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
        Ok(order)
    }


    /// Returns deterministic dependency barriers for potential parallel execution.
    /// Nodes in the same layer have no dependencies on one another. Planning
    /// does not execute callbacks or change the graph.
    pub fn execution_layers(&self) -> Result<Vec<Vec<String>>, OrchestrationError<E>> {
        let order = self.execution_order()?;
        let mut depths: BTreeMap<String, usize> = BTreeMap::new();
        let mut layers: Vec<Vec<String>> = Vec::new();
        for id in order {
            let depth = self.nodes[&id]
                .dependencies
                .iter()
                .map(|dep| depths[dep] + 1)
                .max()
                .unwrap_or(0);
            if layers.len() <= depth {
                layers.resize_with(depth + 1, Vec::new);
            }
            layers[depth].push(id.clone());
            depths.insert(id, depth);
        }
        Ok(layers)
    }

    /// Preflight the complete graph before invoking any node callback.
    pub fn run(&self) -> Result<BTreeMap<String, O>, OrchestrationError<E>>
    where
        O: Clone,
    {
        let order = self.execution_order()?;
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
    fn execution_order_is_stable_and_does_not_execute_nodes() {
        let calls = Arc::new(Mutex::new(0usize));
        let mut graph = FunctionalityOrchestration::new();
        for (id, dependencies) in [("z", vec!["a"]), ("a", vec![]), ("b", vec!["a"])] {
            let calls = Arc::clone(&calls);
            graph.add(node(id, &dependencies, move |_| {
                *calls.lock().unwrap() += 1;
                Ok(1)
            })).unwrap();
        }
        assert_eq!(graph.execution_order().unwrap(), vec!["a", "b", "z"]);
        assert_eq!(*calls.lock().unwrap(), 0);
        assert_eq!(graph.run().unwrap().len(), 3);
        assert_eq!(*calls.lock().unwrap(), 3);
    }

    #[test]
    fn execution_order_rejects_cycles_without_callbacks() {
        let mut graph = FunctionalityOrchestration::new();
        graph.add(node("self", &["self"], |_| panic!("must not execute"))).unwrap();
        assert_eq!(graph.execution_order(), Err(OrchestrationError::Cycle));
        assert_eq!(graph.run(), Err(OrchestrationError::Cycle));
    }


    #[test]
    fn layers_expose_parallel_readiness_without_executing_callbacks() {
        let calls = Arc::new(Mutex::new(0usize));
        let mut graph = FunctionalityOrchestration::new();
        for (id, dependencies) in [
            ("root", vec![]),
            ("independent", vec![]),
            ("left", vec!["root"]),
            ("right", vec!["root"]),
            ("join", vec!["left", "right"]),
            ("tail", vec!["join"]),
        ] {
            let calls = Arc::clone(&calls);
            graph
                .add(node(id, &dependencies, move |_| {
                    *calls.lock().unwrap() += 1;
                    Ok(1)
                }))
                .unwrap();
        }
        assert_eq!(
            graph.execution_layers().unwrap(),
            vec![
                vec!["independent".to_string(), "root".to_string()],
                vec!["left".to_string(), "right".to_string()],
                vec!["join".to_string()],
                vec!["tail".to_string()],
            ]
        );
        assert_eq!(*calls.lock().unwrap(), 0);
        assert_eq!(graph.run().unwrap().len(), 6);
        assert_eq!(*calls.lock().unwrap(), 6);
    }

    #[test]
    fn layers_reject_invalid_graphs_before_execution() {
        let mut missing = FunctionalityOrchestration::new();
        missing.add(node("a", &["absent"], |_| panic!("must not execute"))).unwrap();
        assert_eq!(
            missing.execution_layers(),
            Err(OrchestrationError::MissingDependency {
                node: "a".into(),
                dependency: "absent".into(),
            })
        );
        let mut cycle = FunctionalityOrchestration::new();
        cycle.add(node("a", &["b"], |_| panic!("must not execute"))).unwrap();
        cycle.add(node("b", &["a"], |_| panic!("must not execute"))).unwrap();
        assert_eq!(cycle.execution_layers(), Err(OrchestrationError::Cycle));
    }

    #[test]
    fn duplicate_and_execution_failure_are_explicit() {
        let mut graph = FunctionalityOrchestration::new();
        graph.add(node("a", &[], |_| Err("failed"))).unwrap();
        assert!(matches!(graph.add(node("a", &[], |_| Ok(1))), Err(OrchestrationError::DuplicateNode(id)) if id == "a"));
        assert_eq!(graph.run(), Err(OrchestrationError::Execution { node: "a".into(), error: "failed" }));
    }
}
