//! An observed admission is told its place in line while it waits, once per
//! place, never under the queue's lock.
//!
//! Unix-only for the stand-in `sd-server` file the preflight looks for.

#![cfg(unix)]

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gglib_core::cache_config::CacheRamSetting;
use gglib_core::domain::{ImageFamily, SecondarySlotDecision};
use gglib_core::ports::{AdmitObserver, LaunchOverrides, ModelLaunchSpec};
use gglib_core::server_config::ServerConfigOptions;
use tokio::sync::RwLock;

use super::ResidentSet;
use super::residency_tests::{OneModel, launch_spec};
use crate::process::RuntimeBinaries;
use crate::process::admission::{AdmissionDecision, Candidate};
use crate::process::core::GuiProcessCore;

/// Every place it is told, in order.
#[derive(Debug, Default)]
struct Places(Mutex<Vec<usize>>);

impl AdmitObserver for Places {
    fn queued(&self, position: usize) {
        self.0.lock().unwrap().push(position);
    }
}

fn file(dir: &Path, name: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, b"x").unwrap();
    path
}

/// A request for an image model another request is already launching waits
/// for that launch, and is told it is first in line once, though it asks
/// the queue again every tick.
#[tokio::test]
async fn a_waiting_admission_is_told_its_place_once_per_change() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().unwrap();
    let sd = file(dir.path(), "sd-server");
    let spec = ModelLaunchSpec {
        file_path: file(dir.path(), "sdxl.gguf"),
        image_family: Some(ImageFamily::Sdxl),
        ..launch_spec(999_401, "sdxl")
    };
    let set = Arc::new(ResidentSet::new(
        Arc::new(OneModel(spec)),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
    ));
    let core = Arc::new(RwLock::new(GuiProcessCore::new(
        19_530,
        RuntimeBinaries {
            llama: "/nonexistent/llama-server".into(),
            sd,
        },
    )));
    // Another requester won the launch: the slot is loading for it.
    let first = set.queue().enqueue("sdxl");
    let fits = SecondarySlotDecision::Grant {
        footprint_bytes: 1,
        headroom_bytes: 1 << 40,
    };
    assert!(matches!(
        set.queue().poll(&first, Candidate::image(fits)),
        AdmissionDecision::Launch { .. }
    ));
    drop(first);

    let places = Arc::new(Places::default());
    let observer = Arc::clone(&places) as Arc<dyn AdmitObserver>;
    let task_set = Arc::clone(&set);
    let waiting = tokio::spawn(async move {
        task_set
            .admit_observed(
                &core,
                "sdxl",
                None,
                None,
                LaunchOverrides::default(),
                Some(observer),
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(5), async {
        while places.0.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("told its place");
    // Three more polls at the 250 ms tick, at the same place.
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert!(!waiting.is_finished(), "still waiting for the launch");
    assert_eq!(*places.0.lock().unwrap(), [1]);
    waiting.abort();
}
