//! A plan through the set is the launch the set would make: a pin's
//! overrides reach a plan of the pinned model and no other, the other
//! slots' residents are netted out of the RAM cache as a launch into the
//! primary nets them, and a runtime with its own cache setting plans with
//! it.
//!
//! Unix-only for the stand-in server; not for the behaviour.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;

use gglib_core::cache_config::CacheRamSetting;
use gglib_core::domain::{CacheRamHealth, LaunchPlan, ModelSamplingDefaults, RuntimeKind};
use gglib_core::ports::{
    LaunchOverrides, ModelLaunchSpec, ModelRuntimePort, NoopResidencyObserver, PinnedSpec,
    ServerConfig,
};
use gglib_core::server_config::ServerConfigOptions;
use tokio::sync::RwLock;
use tokio::time::Instant;

use super::ResidentSet;
use super::residency_tests::{StubCatalog, launch_spec};
use crate::ports_impl::RuntimePortImpl;
use crate::process::admission::{PRIMARY_SLOT, Resident};
use crate::process::core::GuiProcessCore;
use crate::process::{ProcessManager, RuntimeBinaries, SpawnConfig};

/// An id no other test in this binary uses: `spawn` writes a pidfile keyed
/// by model id into this binary's data root.
const BESIDE_ID: u32 = 999_601;

fn value_of<'a>(plan: &'a LaunchPlan, flag: &str) -> Option<&'a str> {
    plan.args
        .iter()
        .find(|a| a.flag == flag)
        .and_then(|a| a.value.as_deref())
}

fn plan(set: &ResidentSet, spec: &ModelLaunchSpec) -> LaunchPlan {
    set.plan(spec, None, None, &LaunchOverrides::default())
        .expect("a chat model plans")
}

/// A pin on `qwen2.5` (id 3) with an option of its own reaches a plan of
/// `qwen2.5` and not a plan of `llama-3-8b` (id 7), and never refuses one.
#[test]
fn a_pins_overrides_reach_a_plan_of_the_pinned_model_only() {
    let set = ResidentSet::unobserved(
        Arc::new(StubCatalog),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
    );
    set.set_pin(Some(PinnedSpec {
        id: 3,
        name: "qwen2.5".to_owned(),
        launch_overrides: ServerConfigOptions {
            cache_reuse: Some(123),
            ..Default::default()
        },
    }));

    let pinned = plan(&set, &launch_spec(3, "qwen2.5"));
    let other = plan(&set, &launch_spec(7, "llama-3-8b"));

    assert_eq!(value_of(&pinned, "--cache-reuse"), Some("123"));
    assert_eq!(value_of(&other, "--cache-reuse"), None);
}

/// A stand-in server that sleeps until it is killed.
fn sleeper(dir: &Path) -> std::path::PathBuf {
    let binary = dir.join("server");
    std::fs::write(&binary, "#!/bin/sh\nexec sleep 60\n").unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    binary
}

/// A resident in the second slot weighs on the RAM cache a plan sizes, as
/// it weighs on a launch into the primary; a resident in the primary does
/// not, because that launch would displace it.
#[tokio::test]
async fn the_other_slots_residents_are_netted_out_of_the_plans_ram_cache() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().unwrap();
    let binary = sleeper(dir.path());
    let set = ResidentSet::unobserved(
        Arc::new(StubCatalog),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
    );
    let core = Arc::new(RwLock::new(GuiProcessCore::new(
        19_620,
        RuntimeBinaries {
            llama: binary.clone(),
            sd: binary,
        },
    )));
    let spec = launch_spec(3, "qwen2.5");
    let alone: u64 = value_of(&plan(&set, &spec), "--cache-ram")
        .expect("auto-sized")
        .parse()
        .unwrap();

    // `beside`, 8 GiB of weights, in the second slot.
    let weights = dir.path().join("beside.gguf");
    std::fs::write(&weights, b"x").unwrap();
    let config = ServerConfig::new(i64::from(BESIDE_ID), "beside".to_owned(), weights, 0);
    let (port, _pid) = core
        .write()
        .await
        .spawn(SpawnConfig::Llama(config))
        .expect("the stand-in starts");
    let beside = Resident {
        model_sampling: ModelSamplingDefaults::default(),
        model_id: BESIDE_ID,
        model_name: "beside".to_owned(),
        context_size: 0,
        port,
        projector: None,
        runtime: RuntimeKind::Llama,
        components: Vec::new(),
        slot_restore_supported: true,
        cache_ram_health: CacheRamHealth::LlamaDefault,
        narration: None,
        inflight: 0,
        resident_since: Instant::now(),
        weights_bytes: 8 << 30,
    };
    drop(set.queue().install(1, beside.clone()));
    let with_secondary: u64 = value_of(&plan(&set, &spec), "--cache-ram")
        .expect("auto-sized")
        .parse()
        .unwrap();

    // The same resident in the primary instead: a launch would displace it.
    assert!(set.queue().evict(1).is_some());
    drop(set.queue().install(PRIMARY_SLOT, beside));
    let with_primary: u64 = value_of(&plan(&set, &spec), "--cache-ram")
        .expect("auto-sized")
        .parse()
        .unwrap();
    core.write().await.kill(BESIDE_ID).await.unwrap();

    assert!(
        with_secondary < alone || alone == 0,
        "alone {alone} MiB, beside a second-slot resident {with_secondary} MiB"
    );
    assert_eq!(with_primary, alone, "a primary resident would be displaced");
}

/// A runtime handed its own cache setting (the benchmark's, no prompt
/// cache) plans with it, as it launches with it.
#[test]
fn a_runtime_with_its_own_cache_setting_plans_with_it() {
    let manager = Arc::new(ProcessManager::new(
        9000,
        RuntimeBinaries::llama_only("llama-server"),
        Arc::new(StubCatalog),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
        Arc::new(NoopResidencyObserver),
    ));
    let benchmark =
        RuntimePortImpl::with_cache_ram(Arc::clone(&manager), CacheRamSetting::ExplicitMb(0));
    let proxy = RuntimePortImpl::new(manager);
    let spec = launch_spec(3, "qwen2.5");

    let none = benchmark
        .plan(&spec, None, None, &LaunchOverrides::default())
        .expect("plans");
    let auto = proxy
        .plan(&spec, None, None, &LaunchOverrides::default())
        .expect("plans");

    let cache = none.args.iter().find(|a| a.flag == "--cache-ram").unwrap();
    assert_eq!(cache.value.as_deref(), Some("0"));
    assert_eq!(cache.source, gglib_core::domain::ChoiceSource::You);
    assert_ne!(value_of(&auto, "--cache-ram"), Some("0"), "{auto:?}");
}
