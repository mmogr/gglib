//! Tests for the builtin executor: its table, its refusals, and the file
//! tools run through it.

use super::*;
use gglib_core::ToolCall;

fn make_call(name: &str, args: Value) -> ToolCall {
    ToolCall {
        id: "test-id".to_owned(),
        name: name.to_owned(),
        arguments: args,
    }
}

#[tokio::test]
async fn list_tools_returns_prefixed_names() {
    let adapter = BuiltinToolExecutorAdapter::default();
    let tools = adapter.list_tools().await;
    assert!(!tools.is_empty());
    assert!(tools.iter().all(|t| t.name.starts_with(BUILTIN_PREFIX)));
}

#[tokio::test]
async fn bare_definitions_have_no_prefix() {
    for t in BuiltinToolExecutorAdapter::bare_definitions() {
        assert!(!t.name.starts_with(BUILTIN_PREFIX));
    }
}

#[tokio::test]
async fn get_current_time_human_format_returns_time_object() {
    let adapter = BuiltinToolExecutorAdapter::default();
    let result = adapter
        .execute(&make_call("builtin:get_current_time", json!({})))
        .await
        .unwrap();
    assert!(result.success);
    let parsed: Value = serde_json::from_str(&result.content).unwrap();
    assert!(parsed.get("time").is_some());
    assert!(parsed.get("timezone").is_some());
    assert!(parsed.get("format").is_some());
}

#[tokio::test]
async fn get_current_time_iso_format() {
    let adapter = BuiltinToolExecutorAdapter::default();
    let result = adapter
        .execute(&make_call(
            "builtin:get_current_time",
            json!({ "format": "iso" }),
        ))
        .await
        .unwrap();
    assert!(result.success);
    let parsed: Value = serde_json::from_str(&result.content).unwrap();
    assert_eq!(parsed["format"], "iso");
}

#[tokio::test]
async fn get_current_time_unix_format_is_number() {
    let adapter = BuiltinToolExecutorAdapter::default();
    let result = adapter
        .execute(&make_call(
            "builtin:get_current_time",
            json!({ "format": "unix" }),
        ))
        .await
        .unwrap();
    assert!(result.success);
    let parsed: Value = serde_json::from_str(&result.content).unwrap();
    assert!(parsed["time"].is_number());
}

#[tokio::test]
async fn unknown_tool_returns_error() {
    let adapter = BuiltinToolExecutorAdapter::default();
    let err = adapter
        .execute(&make_call("builtin:nonexistent", json!({})))
        .await;
    assert!(err.is_err());
}

#[test]
fn dropping_the_stop_guard_sets_its_flag() {
    let stop = StopOnDrop::default();
    let flag = Arc::clone(&stop.0);
    assert!(!flag.load(Ordering::Relaxed));
    drop(stop);
    assert!(flag.load(Ordering::Relaxed));
}

#[tokio::test]
async fn file_tools_answer_through_the_executor() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("notes.txt"), "match_me\n").unwrap();
    let adapter = BuiltinToolExecutorAdapter::with_sandbox(dir.path().to_path_buf());

    let read = adapter
        .execute(&make_call(
            "builtin:read_file",
            json!({ "path": "notes.txt" }),
        ))
        .await
        .unwrap();
    assert!(read.success);
    assert_eq!(read.content, "match_me\n");

    let grep = adapter
        .execute(&make_call(
            "builtin:grep_search",
            json!({ "pattern": "match_me" }),
        ))
        .await
        .unwrap();
    assert_eq!(grep.content, "notes.txt:1:match_me");

    let miss = adapter
        .execute(&make_call(
            "builtin:list_directory",
            json!({ "path": "nope" }),
        ))
        .await
        .unwrap();
    assert!(!miss.success);
    assert!(miss.content.contains(sandboxing::MISSING));
}

#[tokio::test]
async fn a_file_tool_without_a_folder_is_refused_without_saying_sandbox() {
    let adapter = BuiltinToolExecutorAdapter::default();
    let err = adapter
        .execute(&make_call("builtin:read_file", json!({ "path": "x" })))
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("working folder"), "{err}");
    assert!(!err.contains("sandbox"), "{err}");
}

#[tokio::test]
async fn missing_prefix_returns_error() {
    let adapter = BuiltinToolExecutorAdapter::default();
    let err = adapter
        .execute(&make_call("get_current_time", json!({})))
        .await;
    assert!(err.is_err());
}
