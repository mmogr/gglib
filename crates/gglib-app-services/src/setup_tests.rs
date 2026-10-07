//! Unit tests for [`super::SetupOps`], and for what the setup status shares
//! with the settings routes: one description of the models directory and one
//! memory reading, under one rule.

use std::sync::Arc;

use super::*;
use crate::settings::{SettingsDeps, SettingsOps};
use crate::test_support::{MockDownloadManager, MockSystemProbePort, test_core};

/// The setup and settings operations over one core and one memory figure.
async fn both_surfaces(total_ram_bytes: u64) -> (SetupOps, SettingsOps) {
    let core = test_core().await;
    let setup = SetupOps::new(SetupDeps {
        core: Arc::clone(&core),
        system_probe: Arc::new(MockSystemProbePort { total_ram_bytes }),
    });
    let settings = SettingsOps::new(SettingsDeps {
        core,
        system_probe: Arc::new(MockSystemProbePort { total_ram_bytes }),
        downloads: Arc::new(MockDownloadManager::new()),
    });
    (setup, settings)
}

#[tokio::test]
async fn get_status_returns_ok_without_panicking() {
    let (ops, _) = both_surfaces(MockSystemProbePort::default().total_ram_bytes).await;
    // get_status calls gglib_runtime directly; we only verify it returns Ok
    // (no panic, no internal unwrap) in a test environment.
    let result = ops.get_status().await;
    assert!(result.is_ok(), "get_status should not fail, got {result:?}");
}

/// 256 MiB is a reading and one byte less is a failed probe, on the settings
/// route and in the setup status alike. The two once disagreed at exactly
/// 256 MiB.
#[tokio::test]
async fn the_memory_floor_is_the_same_in_the_setup_status_and_on_the_settings_route() {
    const FLOOR: u64 = 256 * 1024 * 1024;

    for (total_ram_bytes, readable) in [(FLOOR, true), (FLOOR - 1, false)] {
        let (setup, settings) = both_surfaces(total_ram_bytes).await;

        let in_status = setup.get_status().await.expect("status").system_memory;
        let on_settings = settings.get_system_memory().expect("memory");

        assert_eq!(in_status.is_some(), readable, "{total_ram_bytes} bytes");
        assert_eq!(on_settings.is_some(), readable, "{total_ram_bytes} bytes");
    }
}

/// The status carries the directory and the memory as the settings routes
/// send them, key for key, so there is one shape of each to read.
#[tokio::test]
async fn the_setup_status_sends_the_directory_and_the_memory_the_settings_routes_send() {
    let (setup, settings) = both_surfaces(MockSystemProbePort::default().total_ram_bytes).await;

    let status = serde_json::to_value(setup.get_status().await.expect("status")).expect("json");
    let directory = settings.get_models_directory_info().expect("directory");
    let memory = settings.get_system_memory().expect("memory");

    assert_eq!(
        status["modelsDirectory"],
        serde_json::to_value(directory).expect("json")
    );
    assert_eq!(
        status["systemMemory"],
        serde_json::to_value(memory).expect("json")
    );
    for key in ["path", "source", "default_path", "exists", "writable"] {
        assert!(status["modelsDirectory"].get(key).is_some(), "{key}");
    }
    for key in ["totalRamBytes", "isUnifiedMemory", "hasNvidiaGpu"] {
        assert!(status["systemMemory"].get(key).is_some(), "{key}");
    }
}
