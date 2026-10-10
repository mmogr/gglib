//! [`super::GuiProcessCore::spawn`] starts each runtime from its own binary.
//!
//! Both binaries are shell scripts that write the arguments they were given
//! to a file of their own and then sleep, so a test reads which program ran
//! and with what. Unix-only for the scripts, not for the behaviour.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use gglib_core::domain::{ComponentRole, ImageFamily, ModelComponent, RuntimeKind};
use gglib_core::ports::ServerConfig;

use super::GuiProcessCore;
use crate::pidfile::delete_pidfile;
use crate::process::{RuntimeBinaries, SpawnConfig};
use crate::sd::SdServerConfig;

/// Ids no other test in this binary uses, for the reason
/// `residency::launch_tests` gives.
const LLAMA_ID: i64 = 999_020;
const SD_ID: i64 = 999_021;
const BASE_PORT: u16 = 19_520;

/// A script that writes its arguments, one per line, to `<dir>/<name>.argv`
/// and then sleeps until it is stopped.
fn recorder(dir: &Path, name: &str) -> PathBuf {
    let script = dir.join(name);
    let argv = dir.join(format!("{name}.argv"));
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexec sleep 30\n",
            argv.display()
        ),
    )
    .expect("write script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    script
}

/// What `<dir>/<name>.argv` holds once the script has written it; `None` if
/// it never ran.
async fn recorded(dir: &Path, name: &str) -> Option<Vec<String>> {
    let argv = dir.join(format!("{name}.argv"));
    for _ in 0..100 {
        if let Ok(text) = std::fs::read_to_string(&argv)
            && text.ends_with('\n')
        {
            return Some(text.lines().map(str::to_owned).collect());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    None
}

/// An `sd-server` launch runs the `sd-server` binary with the family's
/// argv, and a llama-server launch runs llama-server; each is listed under
/// its runtime.
#[tokio::test]
async fn each_runtime_runs_its_own_binary() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().expect("temp dir");
    let model = dir.path().join("model.gguf");
    std::fs::write(&model, b"not really a gguf").expect("write model file");
    let mut core = GuiProcessCore::new(
        BASE_PORT,
        RuntimeBinaries {
            llama: recorder(dir.path(), "llama-server"),
            sd: recorder(dir.path(), "sd-server"),
        },
    );

    let sd = SdServerConfig {
        model_id: SD_ID,
        model_name: "flux".to_owned(),
        model_path: model.clone(),
        family: ImageFamily::Flux1,
        components: vec![ModelComponent {
            role: ComponentRole::Vae,
            path: PathBuf::from("/models/ae.safetensors"),
        }],
        port: None,
    };
    let (sd_port, _) = core.spawn(SpawnConfig::Sd(sd)).await.expect("spawn sd");
    let sd_argv = recorded(dir.path(), "sd-server").await;
    let llama_ran_for_sd = dir.path().join("llama-server.argv").exists();

    let llama = ServerConfig::new(LLAMA_ID, "qwen".to_owned(), model.clone(), BASE_PORT);
    core.spawn(SpawnConfig::Llama(llama))
        .await
        .expect("spawn llama");
    let llama_argv = recorded(dir.path(), "llama-server").await;

    let mut listed: Vec<(u32, RuntimeKind)> = core
        .list_all()
        .into_iter()
        .map(|info| (info.model_id, info.runtime))
        .collect();
    listed.sort_unstable_by_key(|(id, _)| *id);

    core.kill(SD_ID as u32).await.ok();
    core.kill(LLAMA_ID as u32).await.ok();
    delete_pidfile(SD_ID).ok();
    delete_pidfile(LLAMA_ID).ok();

    assert!(!llama_ran_for_sd, "an sd launch must not run llama-server");
    let expected_sd: Vec<String> = [
        "--diffusion-model",
        &model.display().to_string(),
        "--vae",
        "/models/ae.safetensors",
        "--listen-ip",
        "127.0.0.1",
        "--listen-port",
        &sd_port.to_string(),
        "--steps",
        "4",
        "--cfg-scale",
        "1",
        "--sampling-method",
        "euler",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    assert_eq!(sd_argv, Some(expected_sd));
    let llama_argv = llama_argv.expect("llama-server ran");
    assert_eq!(
        llama_argv[..2],
        ["-m".to_owned(), model.display().to_string()]
    );
    assert_eq!(
        listed,
        [
            (LLAMA_ID as u32, RuntimeKind::Llama),
            (SD_ID as u32, RuntimeKind::StableDiffusion),
        ]
    );
}

/// With no `sd-server` installed the launch fails, naming the path and the
/// command that installs it, and nothing is tracked.
#[tokio::test]
async fn a_missing_sd_server_names_the_install_command() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().expect("temp dir");
    let model = dir.path().join("model.gguf");
    std::fs::write(&model, b"not really a gguf").expect("write model file");
    let missing = dir.path().join("sd-server");
    let mut core = GuiProcessCore::new(
        BASE_PORT,
        RuntimeBinaries {
            llama: recorder(dir.path(), "llama-server"),
            sd: missing.clone(),
        },
    );
    let sd = SdServerConfig {
        model_id: SD_ID + 10,
        model_name: "sdxl".to_owned(),
        model_path: model,
        family: ImageFamily::Sdxl,
        components: Vec::new(),
        port: None,
    };

    let error = core
        .spawn(SpawnConfig::Sd(sd))
        .await
        .expect_err("no binary, no launch")
        .to_string();

    assert!(error.contains(&missing.display().to_string()), "{error}");
    assert!(error.contains("gglib config sd install"), "{error}");
    assert!(core.list_all().is_empty());
    assert!(!dir.path().join("llama-server.argv").exists());
}
