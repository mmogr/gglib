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

/// 256 MiB is a reading and one byte less is a failed probe, in the setup
/// status and in the reading the download budget falls back to alike. The
/// two once disagreed at exactly 256 MiB.
#[tokio::test]
async fn the_memory_floor_is_the_same_in_the_setup_status_and_the_budgets_reading() {
    const FLOOR: u64 = 256 * 1024 * 1024;

    for (total_ram_bytes, readable) in [(FLOOR, true), (FLOOR - 1, false)] {
        let (setup, _settings) = both_surfaces(total_ram_bytes).await;

        let in_status = setup.get_status().await.expect("status").system_memory;
        let budgets = crate::settings::system_memory(&MockSystemProbePort { total_ram_bytes });

        assert_eq!(in_status.is_some(), readable, "{total_ram_bytes} bytes");
        assert_eq!(budgets.is_some(), readable, "{total_ram_bytes} bytes");
    }
}

/// The status carries the directory as the settings route sends it and the
/// memory as the probe reads it, key for key, so there is one shape of each
/// to read.
#[tokio::test]
async fn the_setup_status_sends_the_directory_and_the_memory_the_settings_routes_send() {
    let (setup, settings) = both_surfaces(MockSystemProbePort::default().total_ram_bytes).await;

    let status = serde_json::to_value(setup.get_status().await.expect("status")).expect("json");
    let directory = settings.get_models_directory_info().expect("directory");
    let memory = crate::settings::system_memory(&MockSystemProbePort::default());

    assert_eq!(
        status["modelsDirectory"],
        serde_json::to_value(directory).expect("json")
    );
    assert_eq!(
        status["systemMemory"],
        serde_json::to_value(memory).expect("json")
    );
    for key in [
        "path",
        "source",
        "default_path",
        "exists",
        "writable",
        "state",
    ] {
        assert!(status["modelsDirectory"].get(key).is_some(), "{key}");
    }
    // The folder's state is the facts beside it, said as the one state
    // every surface words.
    let folder = &status["modelsDirectory"];
    let want = match (folder["exists"].as_bool(), folder["writable"].as_bool()) {
        (Some(false), Some(true)) => "absent",
        (Some(true), Some(true)) => "writable",
        _ => "not_writable",
    };
    assert_eq!(folder["state"], want);
    for key in ["totalRamBytes", "isUnifiedMemory", "hasNvidiaGpu"] {
        assert!(status["systemMemory"].get(key).is_some(), "{key}");
    }
}

/// The wizard reads llama.cpp's standing against its pin from the same place
/// `gglib config llama status` and the Settings card do.
#[tokio::test]
async fn the_setup_status_says_where_llama_stands_as_the_engine_does() {
    let (setup, _) = both_surfaces(MockSystemProbePort::default().total_ram_bytes).await;

    let status = serde_json::to_value(setup.get_status().await.expect("status")).expect("json");
    let engine = gglib_runtime::engine::pin_state(gglib_core::domain::RuntimeKind::Llama)
        .expect("paths resolve");

    assert_eq!(
        status["llamaPin"],
        serde_json::to_value(engine).expect("json")
    );
}

/// An accelerator set up in this test binary's own data root is On in the
/// setup status and in the diagnostics, and Off again once removed: the
/// state is read from the environment on disk, the same file check that
/// picks a download's backend.
#[tokio::test]
async fn the_accelerator_is_on_while_its_environment_is_there_and_off_once_it_is_gone() {
    let (setup, _) = both_surfaces(MockSystemProbePort::default().total_ram_bytes).await;
    let env_dir = gglib_download::cli_exec::fast_helper_status()
        .expect("the accelerator's status")
        .env_dir;
    let python = if cfg!(windows) {
        env_dir.join("Scripts").join("python.exe")
    } else {
        env_dir.join("bin").join("python")
    };
    std::fs::create_dir_all(python.parent().expect("its folder")).expect("the environment");
    std::fs::write(&python, b"").expect("its interpreter");

    let on = serde_json::to_value(setup.get_status().await.expect("status")).expect("json");
    let on_diagnostics = setup.diagnostics().expect("diagnostics").fast_downloads;
    std::fs::remove_dir_all(&env_dir).expect("the environment removed");
    let off = serde_json::to_value(setup.get_status().await.expect("status")).expect("json");
    let off_diagnostics = setup.diagnostics().expect("diagnostics").fast_downloads;

    assert_eq!(on["fastDownloads"], "on");
    assert_eq!(
        on_diagnostics.state,
        gglib_core::domain::FastDownloadsState::On
    );
    assert_eq!(off["fastDownloads"], "off");
    assert_eq!(
        off_diagnostics.state,
        gglib_core::domain::FastDownloadsState::Off
    );
}
