//! The manager's generation gate is the queue's, and `retire_render` stops
//! the image model's server, then empties its slot, then ends the turn.
//!
//! Unix-only for the stand-in `sd-server`, a script that sleeps.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;

use gglib_core::domain::{CacheRamHealth, ImageFamily, ModelSamplingDefaults, RuntimeKind};
use tokio::time::Instant;

use super::*;
use crate::process::SpawnConfig;
use crate::process::residency::residency_tests::StubCatalog;
use crate::sd::SdServerConfig;

/// An id no other test in this binary uses: `spawn` writes a pidfile by id.
const RENDERER_ID: u32 = 999_301;

#[tokio::test]
async fn retiring_a_render_stops_its_server_and_empties_its_slot() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().unwrap();
    let sd = dir.path().join("sd-server");
    std::fs::write(&sd, "#!/bin/sh\nexec sleep 60\n").unwrap();
    std::fs::set_permissions(&sd, std::fs::Permissions::from_mode(0o755)).unwrap();
    let weights = dir.path().join("flux.gguf");
    std::fs::write(&weights, b"x").unwrap();
    let manager = ProcessManager::new(
        19_500,
        RuntimeBinaries {
            llama: "/nonexistent/llama-server".into(),
            sd,
        },
        Arc::new(StubCatalog),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
    );

    let (port, _pid) = manager
        .core
        .write()
        .await
        .spawn(SpawnConfig::Sd(SdServerConfig {
            model_id: i64::from(RENDERER_ID),
            model_name: "flux".to_owned(),
            model_path: weights,
            family: ImageFamily::Flux1,
            components: Vec::new(),
            port: None,
        }))
        .await
        .expect("the stand-in starts");
    let lease = manager.residency.queue().install(
        1,
        Resident {
            model_sampling: ModelSamplingDefaults::default(),
            model_id: RENDERER_ID,
            model_name: "flux".to_owned(),
            context_size: 0,
            port,
            projector: None,
            runtime: RuntimeKind::StableDiffusion,
            components: Vec::new(),
            slot_restore_supported: false,
            cache_ram_health: CacheRamHealth::LlamaDefault,
            narration: None,
            inflight: 0,
            resident_since: Instant::now(),
            weights_bytes: 1,
        },
    );

    let turn = manager
        .generation_gate()
        .render_turn(lease, None)
        .await
        .expect("a lone render is granted at once");
    let retired = manager.retire_render(turn, RENDERER_ID).await;

    assert_eq!(retired.map(|r| r.model_id), Some(RENDERER_ID));
    assert!(
        manager.residency.queue().slot(1).is_none(),
        "the slot is empty"
    );
    assert!(
        !manager.core.read().await.is_running(RENDERER_ID),
        "the server was stopped"
    );
}
