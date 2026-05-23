//! Unit tests for [`TaskGraph::apply_diff`] covering all seven [`GraphDiff`]
//! variants (happy-path) and two invalid-diff error cases.

use gglib_core::domain::orchestrator::task_graph::{
    GraphDiff, HitlMode, NodeId, NodeStatus, TaskGraph, TaskGraphError, TaskNode, TaskNodeKind,
};

// ─── helpers ──────────────────────────────────────────────────────────────────

fn leaf(id: &str, depends_on: &[&str]) -> TaskNode {
    TaskNode {
        id: NodeId(id.into()),
        goal: format!("goal for {id}"),
        depends_on: depends_on.iter().map(|s| NodeId((*s).into())).collect(),
        tool_allowlist: vec![],
        kind: TaskNodeKind::Leaf,
        role: None,
        status: NodeStatus::Pending,
        output: None,
        compacted_output: None,
        error: None,
    }
}

fn simple_graph() -> TaskGraph {
    TaskGraph::new(
        "test graph".into(),
        HitlMode::None,
        vec![leaf("a", &[]), leaf("b", &["a"])],
    )
    .expect("valid graph")
}

// ─── AddNode (happy path) ──────────────────────────────────────────────────

#[test]
fn add_node_appends_node() {
    let mut g = simple_graph();
    let new_node = leaf("c", &["b"]);
    g.apply_diff(&GraphDiff::AddNode { node: new_node }).unwrap();
    assert!(g.nodes.contains_key(&NodeId("c".into())));
}

// ─── RemoveNode (happy path) ───────────────────────────────────────────────

#[test]
fn remove_node_removes_and_strips_edges() {
    let mut g = simple_graph();
    // Remove "a"; "b" depends on "a", so "b"'s depends_on should be cleared.
    g.apply_diff(&GraphDiff::RemoveNode {
        id: NodeId("a".into()),
    })
    .unwrap();
    assert!(!g.nodes.contains_key(&NodeId("a".into())));
    assert!(g.nodes[&NodeId("b".into())].depends_on.is_empty());
}

// ─── SplitNode (happy path) ───────────────────────────────────────────────

#[test]
fn split_node_replaces_original_and_updates_dependents() {
    let mut g = simple_graph();
    let a1 = leaf("a1", &[]);
    let a2 = leaf("a2", &[]);
    g.apply_diff(&GraphDiff::SplitNode {
        id: NodeId("a".into()),
        into: vec![a1, a2],
    })
    .unwrap();
    assert!(!g.nodes.contains_key(&NodeId("a".into())));
    assert!(g.nodes.contains_key(&NodeId("a1".into())));
    assert!(g.nodes.contains_key(&NodeId("a2".into())));
    let b_deps = &g.nodes[&NodeId("b".into())].depends_on;
    assert!(b_deps.contains(&NodeId("a1".into())));
    assert!(b_deps.contains(&NodeId("a2".into())));
}

// ─── RerouteEdge (happy path) ─────────────────────────────────────────────

#[test]
fn reroute_edge_updates_depends_on() {
    let mut g = TaskGraph::new(
        "reroute test".into(),
        HitlMode::None,
        vec![leaf("x", &[]), leaf("y", &[]), leaf("z", &["x"])],
    )
    .unwrap();
    g.apply_diff(&GraphDiff::RerouteEdge {
        node_id: NodeId("z".into()),
        old_dep: NodeId("x".into()),
        new_dep: NodeId("y".into()),
    })
    .unwrap();
    let z_deps = &g.nodes[&NodeId("z".into())].depends_on;
    assert!(!z_deps.contains(&NodeId("x".into())));
    assert!(z_deps.contains(&NodeId("y".into())));
}

// ─── SetRole (happy path) ─────────────────────────────────────────────────

#[test]
fn set_role_updates_node_role() {
    use gglib_core::domain::orchestrator::role_catalog::RoleId;
    let mut g = simple_graph();
    let role = RoleId("analyst".into());
    g.apply_diff(&GraphDiff::SetRole {
        id: NodeId("a".into()),
        role: Some(role.clone()),
    })
    .unwrap();
    assert_eq!(g.nodes[&NodeId("a".into())].role, Some(role));
}

// ─── SetTools (happy path) ────────────────────────────────────────────────

#[test]
fn set_tools_replaces_allowlist() {
    let mut g = simple_graph();
    g.apply_diff(&GraphDiff::SetTools {
        id: NodeId("a".into()),
        tool_allowlist: vec!["search".into(), "calc".into()],
    })
    .unwrap();
    assert_eq!(
        g.nodes[&NodeId("a".into())].tool_allowlist,
        vec!["search".to_string(), "calc".to_string()]
    );
}

// ─── WrapInTeam (happy path) ──────────────────────────────────────────────

#[test]
fn wrap_in_team_creates_team_node() {
    let mut g = simple_graph();
    g.apply_diff(&GraphDiff::WrapInTeam {
        ids: vec![NodeId("a".into()), NodeId("b".into())],
        team_id: NodeId("team_ab".into()),
        team_goal: "team goal".into(),
    })
    .unwrap();
    let team = &g.nodes[&NodeId("team_ab".into())];
    assert!(matches!(team.kind, TaskNodeKind::Team { .. }));
}

// ─── Error: AddNode with duplicate id ────────────────────────────────────

#[test]
fn add_node_duplicate_id_returns_error_and_leaves_graph_unchanged() {
    let mut g = simple_graph();
    let original_len = g.nodes.len();
    let dup = leaf("a", &[]);
    let err = g
        .apply_diff(&GraphDiff::AddNode { node: dup })
        .unwrap_err();
    assert!(matches!(err, TaskGraphError::DuplicateNodeId(_)));
    // Graph must be rolled back.
    assert_eq!(g.nodes.len(), original_len);
}

// ─── Error: RemoveNode that doesn't exist ────────────────────────────────

#[test]
fn remove_nonexistent_node_returns_not_found() {
    let mut g = simple_graph();
    let err = g
        .apply_diff(&GraphDiff::RemoveNode {
            id: NodeId("does_not_exist".into()),
        })
        .unwrap_err();
    assert!(matches!(err, TaskGraphError::NodeNotFound(_)));
}
