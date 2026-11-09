//! Each state's wire name and word, and the rules that pick a state.

use super::*;

#[test]
fn a_listening_proxy_is_on_or_pinned_and_a_resident_works_while_it_answers() {
    assert_eq!(EndpointState::listening(false), EndpointState::On);
    assert_eq!(EndpointState::listening(true), EndpointState::Pinned);
    assert_eq!(ResidencyState::of_inflight(0), ResidencyState::Loaded);
    assert_eq!(ResidencyState::of_inflight(2), ResidencyState::Working);
    assert_eq!(ReachState::of(false, true), ReachState::Disconnected);
    assert_eq!(ReachState::of(true, true), ReachState::Away);
    assert_eq!(ReachState::of(true, false), ReachState::Connected);
    assert_eq!(KeyState::of(true), KeyState::Required);
    assert_eq!(KeyState::of(false), KeyState::NoneNeeded);
}

#[test]
fn every_state_is_on_the_wire_as_its_snake_case_name_and_reads_as_its_word() {
    for state in EndpointState::ALL {
        let wire = serde_json::to_string(&state).unwrap();
        assert_eq!(wire, format!("\"{}\"", state.word().to_lowercase()));
        assert_eq!(state.to_string(), state.word());
    }
    assert_eq!(
        serde_json::to_string(&ResidencyState::NotLoaded).unwrap(),
        "\"not_loaded\""
    );
    assert_eq!(ResidencyState::NotLoaded.to_string(), "Not loaded");
    assert_eq!(ReachState::default(), ReachState::Disconnected);
    assert_eq!(
        serde_json::to_string(&KeyState::NoneNeeded).unwrap(),
        "\"none_needed\""
    );
    assert_eq!(KeyState::NoneNeeded.to_string(), "None needed");
}

#[test]
fn an_engine_is_installed_not_working_or_not_installed() {
    assert_eq!(EngineState::of(false, true), EngineState::NotInstalled);
    assert_eq!(EngineState::of(true, true), EngineState::NotWorking);
    assert_eq!(EngineState::of(true, false), EngineState::Installed);
    assert_eq!(EngineState::default(), EngineState::NotInstalled);
    assert_eq!(EngineState::Installed.word(), "Installed");
    assert_eq!(EngineState::NotWorking.word(), "Installed, not working");
    assert_eq!(EngineState::NotInstalled.to_string(), "Not installed");
    assert_eq!(
        serde_json::to_string(&EngineState::NotWorking).unwrap(),
        "\"not_working\""
    );
}

#[test]
fn a_tool_server_that_errored_is_failed_and_one_gglib_cannot_run_is_not_supported() {
    let of = |status: McpServerStatus| McpState::of(&status);
    assert_eq!(of(McpServerStatus::Running), McpState::Running);
    assert_eq!(of(McpServerStatus::Starting), McpState::Starting);
    assert_eq!(of(McpServerStatus::Stopped), McpState::Stopped);
    assert_eq!(
        of(McpServerStatus::Error("spawn failed".into())),
        McpState::Failed
    );
    assert_eq!(of(McpServerStatus::Unsupported), McpState::Unsupported);
    assert_eq!(McpState::Running.word(), "Running");
    assert_eq!(McpState::Starting.word(), "Starting");
    assert_eq!(McpState::Stopped.word(), "Stopped");
    assert_eq!(McpState::Failed.to_string(), "Failed");
    assert_eq!(McpState::Unsupported.word(), "Not supported yet");
}

#[test]
fn the_accelerator_is_on_when_provisioned_and_the_folder_says_whether_it_can_be_written() {
    assert_eq!(FastDownloadsState::of(true), FastDownloadsState::On);
    assert_eq!(FastDownloadsState::of(false), FastDownloadsState::Off);
    assert_eq!(FastDownloadsState::On.word(), "On");
    assert_eq!(FastDownloadsState::Off.to_string(), "Off");
    assert_eq!(ModelsDirState::of(false, true), ModelsDirState::Absent);
    assert_eq!(ModelsDirState::of(true, true), ModelsDirState::Writable);
    assert_eq!(ModelsDirState::of(true, false), ModelsDirState::NotWritable);
    assert_eq!(ModelsDirState::default(), ModelsDirState::Absent);
    assert_eq!(ModelsDirState::Writable.word(), "Writable");
    assert_eq!(ModelsDirState::Absent.word(), "Will be created");
    assert_eq!(ModelsDirState::NotWritable.to_string(), "Not writable");
    assert_eq!(
        serde_json::to_string(&ModelsDirState::NotWritable).unwrap(),
        "\"not_writable\""
    );
}

#[test]
fn a_pinned_endpoint_is_said_with_its_model() {
    assert_eq!(EndpointState::pinned_to("qwen3-8b"), "Pinned to qwen3-8b");
}
