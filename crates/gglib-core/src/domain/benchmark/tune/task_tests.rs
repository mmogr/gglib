//! Tests for [`super::task`]: the task-suite schema, and which of the
//! built-in multi-turn tasks genuinely demand a second turn.

use super::*;

/// `ExpectedOutcome` is `#[serde(tag = "kind")]` (internally tagged), which
/// only supports newtype variants whose inner value serializes as a JSON
/// object/map. `ToolCalls` must therefore stay a *struct* variant
/// (`{ calls: Vec<..> }`), never a bare `ToolCalls(Vec<..>)` newtype —
/// the latter fails at serialization time with "cannot serialize tagged
/// newtype variant ... containing a sequence".
#[test]
fn expected_outcome_tool_calls_round_trips() {
    let outcome = ExpectedOutcome::ToolCalls {
        calls: vec![ExpectedCall {
            name: "get_weather".to_string(),
            required_args: serde_json::Map::new(),
            ordered: false,
            depends_on_result: false,
        }],
    };
    let json = serde_json::to_string(&outcome).expect("serializes");
    let round_tripped: ExpectedOutcome = serde_json::from_str(&json).expect("deserializes");
    assert!(matches!(round_tripped, ExpectedOutcome::ToolCalls { .. }));
}

/// Which multi-turn tasks demand a second turn is a judgement about each
/// task, not a property of the category — so it is pinned here rather than
/// left to whoever next reads the suite and notices the inconsistency.
///
/// `create_then_append` is the deliberate exception: appending to a path
/// you already know needs no intervening result, so a model that does both
/// at once is being more efficient rather than skipping a step. Marking it
/// would delete a real finding — the gglib arm fixes that task 0/3 → 3/3.
#[test]
fn only_the_genuinely_dependent_multi_turn_tasks_demand_a_second_turn() {
    let tasks = TaskSuite::Default.resolve().expect("default suite parses");
    let dependent = |id: &str| {
        let task = tasks
            .iter()
            .find(|t| t.id == id)
            .unwrap_or_else(|| panic!("{id} missing from the default suite"));
        match &task.expected {
            ExpectedOutcome::ToolCalls { calls } => calls.iter().any(|c| c.depends_on_result),
            ExpectedOutcome::NoToolCall => false,
        }
    };

    assert!(
        dependent("multi_turn_search_then_read"),
        "read_file's path comes out of the search results"
    );
    assert!(
        dependent("multi_turn_check_then_delete"),
        "the delete is conditional on what the existence check returned"
    );
    assert!(
        !dependent("multi_turn_create_then_append"),
        "appending to a path you just created needs no result — one batch is \
         a better answer, not a skipped step"
    );
}

#[test]
fn expected_outcome_no_tool_call_round_trips() {
    let json = serde_json::to_string(&ExpectedOutcome::NoToolCall).expect("serializes");
    let round_tripped: ExpectedOutcome = serde_json::from_str(&json).expect("deserializes");
    assert!(matches!(round_tripped, ExpectedOutcome::NoToolCall));
}

#[test]
fn task_suite_custom_round_trips() {
    let suite = TaskSuite::Custom {
        tasks: vec![TuneTask {
            id: "single_call_example".to_string(),
            category: TaskCategory::SingleCall,
            system_prompt: None,
            history: None,
            user_prompt: "What's the weather in Boston?".to_string(),
            tools: vec![],
            expected: ExpectedOutcome::NoToolCall,
        }],
    };
    let json = serde_json::to_string(&suite).expect("serializes");
    let round_tripped: TaskSuite = serde_json::from_str(&json).expect("deserializes");
    assert!(matches!(round_tripped, TaskSuite::Custom { .. }));
}

/// Guards the embedded default suite asset: it must always parse, and
/// must cover all five categories so the pre-screen round (which picks
/// one `SingleCall` + one `Irrelevance` task) always has candidates to
/// draw from, and the endurance scenario is never silently dropped.
#[test]
fn default_suite_parses_and_covers_all_categories() {
    let tasks = TaskSuite::Default.resolve().expect("embedded suite parses");
    assert!(!tasks.is_empty(), "default suite must not be empty");

    for category in [
        TaskCategory::SingleCall,
        TaskCategory::ParallelCall,
        TaskCategory::MultiTurn,
        TaskCategory::Irrelevance,
        TaskCategory::LongContext,
    ] {
        assert!(
            tasks.iter().any(|t| t.category == category),
            "default suite missing a task in category {category:?}"
        );
    }

    // Task IDs must be unique — the tune service keys results by ID.
    let mut ids: Vec<&str> = tasks.iter().map(|t| t.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(
        ids.len(),
        tasks.len(),
        "default suite has duplicate task IDs"
    );
}

/// The long-context task must actually carry a non-trivial pre-filled
/// history — otherwise it's indistinguishable from a cold-start task and
/// defeats the purpose of the category.
#[test]
fn long_context_task_has_substantial_history() {
    let tasks = TaskSuite::Default.resolve().expect("embedded suite parses");
    let long_context_tasks: Vec<_> = tasks
        .iter()
        .filter(|t| t.category == TaskCategory::LongContext)
        .collect();
    assert!(
        !long_context_tasks.is_empty(),
        "expected at least one long_context task"
    );
    for task in long_context_tasks {
        let history = task
            .history
            .as_ref()
            .expect("long_context task must set history");
        assert!(
            history.len() >= 8,
            "long_context task '{}' history too short ({} messages) to \
             meaningfully simulate context degradation",
            task.id,
            history.len()
        );
    }
}
