//! Pinned KrnlTS seven-state transition fixture; exhaustive matrix.
use krnlrs::lifecycle::{Lifecycle, LifecycleState};

fn state(name: &str) -> LifecycleState {
    use LifecycleState::*;
    match name {
        "uninitialized" => Uninitialized,
        "initializing" => Initializing,
        "ready" => Ready,
        "starting" => Starting,
        "running" => Running,
        "stopping" => Stopping,
        "stopped" => Stopped,
        _ => panic!("unknown fixture state: {name}"),
    }
}

fn at_state(target: LifecycleState) -> Lifecycle {
    use LifecycleState::*;
    let path = match target {
        Uninitialized => vec![],
        Initializing => vec![Initializing],
        Ready => vec![Initializing, Ready],
        Starting => vec![Initializing, Ready, Starting],
        Running => vec![Initializing, Ready, Starting, Running],
        Stopping => vec![Initializing, Ready, Starting, Running, Stopping],
        Stopped => vec![Initializing, Ready, Starting, Running, Stopping, Stopped],
    };
    let mut lifecycle = Lifecycle::new();
    for step in path {
        lifecycle.transition(step).unwrap();
    }
    lifecycle
}

#[test]
fn pinned_upstream_all_49_transitions_match_and_reject_without_mutation() {
    let fixture = include_str!("../testdata/upstream-golden/lifecycle.tsv");
    let mut count = 0usize;
    for row in fixture.lines().filter(|line| !line.starts_with('#') && !line.trim().is_empty()) {
        let parts: Vec<&str> = row.split('\t').collect();
        assert_eq!(parts.len(), 3);
        let from = state(parts[0]);
        let to = state(parts[1]);
        let should_allow = parts[2] == "allow";
        let mut lifecycle = at_state(from);
        let result = lifecycle.transition(to);
        assert_eq!(result.is_ok(), should_allow, "{row}");
        assert_eq!(lifecycle.state(), if should_allow { to } else { from }, "{row}");
        count += 1;
    }
    assert_eq!(count, 49);
}
