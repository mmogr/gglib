//! Shared DAG presentation helpers for task-graph rendering.
//!
//! Used by both the `plan` command (stdout) and the `council` command
//! (stderr at `PlanProposed`) so the tree and Mermaid output are identical
//! regardless of entry point.
//!
//! The caller chooses the output channel by passing `&mut impl Write`:
//!
//! ```ignore
//! // plan command  — primary output on stdout
//! dag::render_tree(&graph, &mut std::io::stdout());
//!
//! // council command — informational on stderr
//! dag::render_tree(graph, &mut std::io::stderr());
//! ```

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Write;

use gglib_core::domain::council::task_graph::{NodeId, TaskGraph};

use crate::presentation::style;

// ─── Per-node colouring ──────────────────────────────────────────────────────

/// ANSI foreground-colour palette used to colour node-id labels.
///
/// Entries avoid plain red/green/blue so they do not clash with the semantic
/// colours in [`style`] (DANGER = red, SUCCESS = green, INFO = blue).
/// All entries are readable against both dark and light terminal backgrounds.
const NODE_PALETTE: &[&str] = &[
    "\x1b[36m", // Cyan
    "\x1b[35m", // Magenta
    "\x1b[33m", // Yellow
    "\x1b[94m", // Bright Blue
    "\x1b[96m", // Bright Cyan
    "\x1b[95m", // Bright Magenta
    "\x1b[93m", // Bright Yellow
    "\x1b[92m", // Bright Green
    "\x1b[91m", // Bright Red
    "\x1b[97m", // Bright White
];

/// Return a stable ANSI foreground-colour escape for a given node id string.
///
/// The colour is derived from a simple byte-sum checksum, so the same node id
/// always maps to the same colour across all events and re-runs.
pub fn node_color(node_id: &str) -> &'static str {
    let hash: usize = node_id.bytes().map(|b| b as usize).sum();
    NODE_PALETTE[hash % NODE_PALETTE.len()]
}

// ─── Tree rendering ──────────────────────────────────────────────────────────

/// Render the task graph as an indented tree.
///
/// Node-id labels are coloured with [`node_color`] so they match the colours
/// used in the live-streaming council output.
///
/// # Format
///
/// ```text
/// ── goal: Research the history of llama.cpp and write a summary
///    ├── [research] Research the history and development…
///    └── [write-summary] Write a clear, comprehensive… (needs: research)
/// ```
pub fn render_tree(graph: &TaskGraph, out: &mut impl Write) {
    let _ = writeln!(
        out,
        "{}── goal:{} {}",
        style::BOLD,
        style::RESET,
        graph.goal
    );

    let ordered = topological_order(graph);
    let last = ordered.len().saturating_sub(1);

    for (i, id) in ordered.iter().enumerate() {
        let node = &graph.nodes[id];
        let connector = if i == last {
            "   └──"
        } else {
            "   ├──"
        };
        let color = node_color(&id.0);
        let deps = if node.depends_on.is_empty() {
            String::new()
        } else {
            let dep_ids: Vec<&str> = node.depends_on.iter().map(|d| d.0.as_str()).collect();
            format!(
                " {}(needs: {}){}",
                style::DIM,
                dep_ids.join(", "),
                style::RESET,
            )
        };
        let _ = writeln!(
            out,
            "{} {}[{}]{} {}{}",
            connector,
            color,
            id,
            style::RESET,
            node.goal,
            deps,
        );
    }
}

// ─── Mermaid rendering ───────────────────────────────────────────────────────

/// Render the task graph as a Mermaid flowchart diagram.
///
/// Node goals are truncated to 50 characters for readability.
///
/// # Format
///
/// ````text
/// ```mermaid
/// flowchart LR
///     research["Research the history…"]
///     write-summary["Write a clear…"]
///     research --> write-summary
/// ```
/// ````
pub fn render_mermaid(graph: &TaskGraph, out: &mut impl Write) {
    let _ = writeln!(out, "```mermaid");
    let _ = writeln!(out, "flowchart LR");

    for (id, node) in &graph.nodes {
        let label = if node.goal.len() > 50 {
            format!("{}…", &node.goal[..50])
        } else {
            node.goal.clone()
        };
        let label = label.replace('"', "'");
        let _ = writeln!(out, "    {}[\"{}\"]", id, label);
    }

    for (id, node) in &graph.nodes {
        for dep in &node.depends_on {
            let _ = writeln!(out, "    {} --> {}", dep, id);
        }
    }

    let _ = writeln!(out, "```");
}

// ─── Topological ordering ─────────────────────────────────────────────────────

/// Return node ids in topological (dependency-first) order.
///
/// Uses Kahn's algorithm. Nodes at the same depth level are sorted
/// alphabetically so the output is deterministic regardless of the
/// underlying map order.
pub fn topological_order(graph: &TaskGraph) -> Vec<&NodeId> {
    let mut in_degree: HashMap<&NodeId, usize> = graph.nodes.keys().map(|id| (id, 0)).collect();
    let mut dependents: HashMap<&NodeId, Vec<&NodeId>> = HashMap::new();

    for (id, node) in &graph.nodes {
        for dep in &node.depends_on {
            *in_degree.entry(id).or_insert(0) += 1;
            dependents.entry(dep).or_default().push(id);
        }
    }

    let mut queue_vec: Vec<&NodeId> = in_degree
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(&id, _)| id)
        .collect();
    queue_vec.sort_by_key(|id| &id.0);
    let mut queue: VecDeque<&NodeId> = queue_vec.into_iter().collect();

    let mut result = Vec::new();
    let mut visited: HashSet<&NodeId> = HashSet::new();

    while let Some(id) = queue.pop_front() {
        if visited.contains(id) {
            continue;
        }
        visited.insert(id);
        result.push(id);

        if let Some(children) = dependents.get(id) {
            let mut children: Vec<&NodeId> = children.clone();
            children.sort_by_key(|id| &id.0);
            for child in children {
                let entry = in_degree.entry(child).or_insert(0);
                if *entry > 0 {
                    *entry -= 1;
                }
                if *entry == 0 && !visited.contains(child) {
                    queue.push_back(child);
                }
            }
        }
    }

    result
}
