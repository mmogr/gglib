//! The image runtime's status and removal, with a stand-in runtime: what an
//! install would download and what to say about it, what is running on
//! `sd-server`, and the refusal to remove it from under a running model.
//! The one test that reaches `.sd/` reaches this binary's own data root.

use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::ports::{
    Admission, LaunchOverrides, ModelRuntimeError, ProcessHandle, RunningTarget,
};
use gglib_runtime::engine::{PinStanding, PinState};
use gglib_runtime::sd::PINNED_SD_RELEASE;

use super::*;
use crate::setup::SetupDeps;
use crate::setup::{refuse_while_serving, running_model};
use crate::test_support::{MockSystemProbePort, test_core};

/// A runtime serving the models given, each `(name, runtime)`.
#[derive(Debug)]
struct Serving(Vec<(&'static str, RuntimeKind)>);

#[async_trait]
impl ModelRuntimePort for Serving {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        unimplemented!("a status and a removal start nothing")
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn list_running(&self) -> Vec<ProcessHandle> {
        self.0
            .iter()
            .zip(1_i64..)
            .map(|(&(name, runtime), id)| {
                ProcessHandle::new(id, name.to_owned(), None, 9000, 0).with_runtime(runtime)
            })
            .collect()
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }

    async fn stop_model(&self, _model_id: u32) -> Result<bool, ModelRuntimeError> {
        Ok(false)
    }
}

/// Nothing installed, as a fresh machine reports it.
fn nothing_installed() -> SdStatus {
    SdStatus {
        installed: false,
        binary_path: "/data/.sd/bin/sd-server".to_owned(),
        config_path: "/data/.sd/sd-config.json".to_owned(),
        pinned_release: PINNED_SD_RELEASE.to_owned(),
        install_type: None,
        release: None,
        platform: None,
        installed_at: None,
        record_error: None,
        version_line: None,
        commit: None,
        pin: PinState {
            standing: PinStanding::Missing,
            installed_release: None,
            target_release: Some(PINNED_SD_RELEASE.to_owned()),
            built: None,
            rebuilds: false,
            update_downloads: false,
        },
    }
}

#[test]
fn a_cpu_build_is_offered_with_its_warning() {
    let warning = "Images will take minutes each.";

    let status = status_of(
        nothing_installed(),
        Ok(("Linux x64 (CPU)", Some(warning))),
        None,
    );

    assert_eq!(status.prebuilt.as_deref(), Some("Linux x64 (CPU)"));
    assert_eq!(status.warning.as_deref(), Some(warning));
    assert_eq!(status.prebuilt_unavailable, None);
    assert_eq!(status.install_command, "gglib config sd install");
}

#[test]
fn a_platform_with_no_prebuilt_build_says_why_and_names_the_command() {
    let reason = "stable-diffusion.cpp publishes no pre-built linux build for aarch64";

    let status = status_of(nothing_installed(), Err(reason.to_owned()), None);

    assert_eq!(status.prebuilt, None);
    assert_eq!(status.warning, None);
    assert_eq!(status.prebuilt_unavailable.as_deref(), Some(reason));
    assert_eq!(status.install_command, SD_INSTALL_COMMAND);
}

/// The wire shape the web reads, keys included.
#[test]
fn the_status_serializes_in_camel_case_with_the_install_nested() {
    let status = status_of(
        nothing_installed(),
        Ok(("macOS universal (Metal)", None)),
        Some("flux".to_owned()),
    );

    let json = serde_json::to_value(&status).unwrap();

    assert_eq!(json["install"]["installed"], false);
    assert_eq!(json["install"]["pinnedRelease"], PINNED_SD_RELEASE);
    assert_eq!(json["prebuilt"], "macOS universal (Metal)");
    assert_eq!(json["prebuiltUnavailable"], serde_json::Value::Null);
    assert_eq!(json["runningModel"], "flux");
    assert_eq!(json["installCommand"], SD_INSTALL_COMMAND);
}

#[tokio::test]
async fn the_running_image_model_is_the_one_on_sd_server() {
    let chat_only = Serving(vec![("qwen", RuntimeKind::Llama)]);
    let both = Serving(vec![
        ("qwen", RuntimeKind::Llama),
        ("flux", RuntimeKind::StableDiffusion),
    ]);

    assert_eq!(
        running_model(&chat_only, RuntimeKind::StableDiffusion).await,
        None
    );
    assert_eq!(
        running_model(&both, RuntimeKind::StableDiffusion)
            .await
            .as_deref(),
        Some("flux")
    );
    assert_eq!(
        running_model(&both, RuntimeKind::Llama).await.as_deref(),
        Some("qwen")
    );
}

#[tokio::test]
async fn removal_is_refused_while_an_image_model_runs_and_names_it() {
    let drawing = Serving(vec![("flux", RuntimeKind::StableDiffusion)]);

    let refused = refuse_while_serving(&drawing, RuntimeKind::StableDiffusion, "removing")
        .await
        .unwrap_err();

    match refused {
        GuiError::Conflict(message) => assert_eq!(
            message,
            "flux is running on sd-server. Stop it before removing stable-diffusion.cpp."
        ),
        other => panic!("expected a conflict, got {other:?}"),
    }
}

#[tokio::test]
async fn removal_is_not_refused_for_a_chat_model() {
    let chatting = Serving(vec![("qwen", RuntimeKind::Llama)]);

    assert!(
        refuse_while_serving(&chatting, RuntimeKind::StableDiffusion, "removing")
            .await
            .is_ok()
    );
}

/// The removal itself refuses while an image model runs, and removes
/// nothing: the `sd-server` installed under this binary's data root stays.
#[tokio::test]
async fn uninstall_refuses_while_an_image_model_runs_and_removes_nothing() {
    let root = gglib_core::paths::isolate_data_root();
    let binary = gglib_core::paths::sd_server_path().unwrap();
    assert!(binary.starts_with(root), "{}", binary.display());
    std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
    std::fs::write(&binary, b"#!/bin/sh\n").unwrap();
    let setup = SetupOps::new(SetupDeps {
        core: test_core().await,
        system_probe: Arc::new(MockSystemProbePort::default()),
    });
    let drawing = Serving(vec![
        ("qwen", RuntimeKind::Llama),
        ("flux", RuntimeKind::StableDiffusion),
    ]);

    let refused = setup.uninstall_sd(&drawing).await.unwrap_err();

    assert!(
        matches!(&refused, GuiError::Conflict(message) if message.contains("flux")),
        "{refused:?}"
    );
    assert!(binary.exists(), "nothing was removed");
}

/// An install is refused under a running model on its own engine, for
/// either engine, before anything is fetched.
#[tokio::test]
async fn an_install_is_refused_while_a_model_runs_on_its_engine() {
    let setup = SetupOps::new(SetupDeps {
        core: test_core().await,
        system_probe: Arc::new(MockSystemProbePort::default()),
    });
    let both = Serving(vec![
        ("qwen", RuntimeKind::Llama),
        ("flux", RuntimeKind::StableDiffusion),
    ]);
    let (tx, _rx) = tokio::sync::mpsc::channel(8);

    let llama = setup.install_llama(&both, tx.clone()).await.unwrap_err();
    let sd = setup.install_sd(&both, tx).await.unwrap_err();

    assert!(
        matches!(&llama, GuiError::Conflict(m) if m == "qwen is running on llama-server. Stop it before installing llama.cpp."),
        "{llama:?}"
    );
    assert!(
        matches!(&sd, GuiError::Conflict(m) if m == "flux is running on sd-server. Stop it before installing stable-diffusion.cpp."),
        "{sd:?}"
    );
}

/// llama.cpp's removal refuses under a chat model as the image runtime's
/// does under an image model, and removes nothing.
#[tokio::test]
async fn llama_uninstall_refuses_while_a_chat_model_runs() {
    let root = gglib_core::paths::isolate_data_root();
    let binary = gglib_core::paths::llama_server_path().unwrap();
    assert!(binary.starts_with(root), "{}", binary.display());
    std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
    std::fs::write(&binary, b"#!/bin/sh\n").unwrap();
    let setup = SetupOps::new(SetupDeps {
        core: test_core().await,
        system_probe: Arc::new(MockSystemProbePort::default()),
    });

    let refused = setup
        .uninstall_llama(&Serving(vec![("qwen", RuntimeKind::Llama)]))
        .await
        .unwrap_err();

    assert!(
        matches!(&refused, GuiError::Conflict(m) if m.contains("qwen")),
        "{refused:?}"
    );
    assert!(binary.exists(), "nothing was removed");
}

/// An install the command line would do as a source build is refused, not
/// run as a download: here a source build of the pinned release is there,
/// and downloading over it is not the daemon's to choose.
#[tokio::test]
async fn an_install_over_a_source_build_of_a_release_is_left_to_the_terminal() {
    let root = gglib_core::paths::isolate_data_root();
    let binary = gglib_core::paths::sd_server_path().unwrap();
    assert!(binary.starts_with(root), "{}", binary.display());
    std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
    std::fs::write(&binary, b"#!/bin/sh\n").unwrap();
    std::fs::write(
        gglib_core::paths::sd_config_path().unwrap(),
        format!(
            r#"{{"version":"{PINNED_SD_RELEASE}","commit_sha":"228c707aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","build_date":"2026-10-10T00:00:00Z","acceleration":"Metal","cmake_flags":["-DSD_METAL=ON"]}}"#
        ),
    )
    .unwrap();
    let setup = SetupOps::new(SetupDeps {
        core: test_core().await,
        system_probe: Arc::new(MockSystemProbePort::default()),
    });
    let (tx, _rx) = tokio::sync::mpsc::channel(8);

    let refused = setup
        .install_sd(&Serving(Vec::new()), tx)
        .await
        .unwrap_err();

    assert!(
        matches!(&refused, GuiError::Conflict(m) if m == "stable-diffusion.cpp was built from source here, so it is updated from a terminal, which builds it again: run 'gglib config sd update'."),
        "{refused:?}"
    );
}
