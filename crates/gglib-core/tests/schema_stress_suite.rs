//! `docs/benchmark/schema_stress_suite.json` is a suite repair can judge.
//!
//! The suite exists to be violated: nested required objects, enums, an
//! integer a model is tempted to quote, a parameter named `if`, arrays of
//! objects. It is only worth running through the proxy arm if the proxy's
//! validator can read every schema in it. A schema it cannot read comes back
//! `Unvalidatable`, and the proxy forwards the call untouched, so a suite
//! with one would measure nothing on that task and look like a model that
//! never needed repair.
//!
//! So each task is checked both ways against the validator the proxy runs:
//! the call its expected arguments describe is `Valid`, and the same call
//! with one required argument taken out is `Invalid` — not `Unvalidatable`.

use std::path::PathBuf;

use gglib_core::domain::benchmark::tune::task::{ExpectedOutcome, TuneTask};
use gglib_core::request_pipeline::{Verdict, validate_tool_calls};
use serde_json::{Map, Value, json};

fn suite() -> Vec<TuneTask> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/benchmark/schema_stress_suite.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} should be readable: {e}", path.display()));
    serde_json::from_str(&text).expect("the suite parses as tune tasks")
}

/// The request's `tools`, in the `OpenAI` shape the proxy validates against.
fn tools(task: &TuneTask) -> Value {
    Value::Array(
        task.tools
            .iter()
            .map(|t| {
                json!({"type": "function", "function": {
                    "name": t.name,
                    "parameters": t.input_schema,
                }})
            })
            .collect(),
    )
}

fn call(name: &str, arguments: &Map<String, Value>) -> Value {
    json!([{"id": "call_0", "type": "function", "function": {
        "name": name,
        "arguments": Value::Object(arguments.clone()).to_string(),
    }}])
}

/// Each task's one expected call: its tool name and arguments.
fn expected(task: &TuneTask) -> (&str, &Map<String, Value>) {
    let ExpectedOutcome::ToolCalls { calls } = &task.expected else {
        panic!("{}: every task in this suite expects a call", task.id);
    };
    assert_eq!(calls.len(), 1, "{}: one call per task", task.id);
    (calls[0].name.as_str(), &calls[0].required_args)
}

#[test]
fn every_expected_call_fits_its_schema() {
    let suite = suite();
    assert_eq!(suite.len(), 6, "the suite's six tasks");
    for task in &suite {
        let (name, args) = expected(task);
        let verdict = validate_tool_calls(Some(&tools(task)), Some(&call(name, args)));
        assert_eq!(verdict, Verdict::Valid, "{}: {verdict:?}", task.id);
    }
}

#[test]
fn every_schema_is_one_the_validator_can_judge() {
    for task in &suite() {
        let (name, args) = expected(task);
        let dropped = args.keys().next().expect("an argument to drop").clone();
        let mut broken = args.clone();
        broken.remove(&dropped);
        let verdict = validate_tool_calls(Some(&tools(task)), Some(&call(name, &broken)));
        assert!(
            matches!(verdict, Verdict::Invalid(_)),
            "{}: dropping `{dropped}` should be caught, got {verdict:?}",
            task.id
        );
    }
}
