//! An image model's launch narrates its runtime build, its family, every
//! component it draws with, the slot it was placed in and the memory that
//! placement was judged by.

use std::path::PathBuf;

use gglib_core::domain::{
    ComponentRole, ImageFamily, LaunchNarration, ModelComponent, ModelSamplingDefaults,
    SlotFootprint,
};
use gglib_core::ports::ModelLaunchSpec;

use super::narrate_sd_with;
use crate::process::admission::PRIMARY_SLOT;

const GIB: u64 = 1024 * 1024 * 1024;

fn flux() -> ModelLaunchSpec {
    let component = |role, file: &str| ModelComponent {
        role,
        path: PathBuf::from("/models/flux").join(file),
    };
    ModelLaunchSpec {
        model_sampling: ModelSamplingDefaults::default(),
        id: 12,
        name: "flux1-schnell".to_owned(),
        file_path: "/models/flux/flux1-schnell-q8_0.gguf".into(),
        projector: None,
        image_family: Some(ImageFamily::Flux1),
        // Listed out of role order on purpose.
        components: vec![
            component(ComponentRole::T5xxl, "t5xxl_fp16.safetensors"),
            component(ComponentRole::Vae, "ae.safetensors"),
            component(ComponentRole::ClipL, "clip_l.safetensors"),
        ],
        tags: Vec::new(),
        architecture: None,
        quantization: Some("Q8_0".to_owned()),
        context_length: None,
        server_defaults: None,
        file_size_bytes: 23 * GIB,
        kv_elems_per_token: None,
        kv_memory_is_partial: false,
    }
}

fn value(n: &LaunchNarration, label: &str) -> String {
    n.decision(label)
        .unwrap_or_else(|| panic!("no {label} line in {:?}", n.decisions))
        .value
        .clone()
}

#[test]
fn an_sd_launch_names_runtime_family_components_slot_and_memory() {
    let footprint = SlotFootprint {
        weights_bytes: 30 * GIB,
        kv_bytes: 0,
    };
    let n = narrate_sd_with(&flux(), Some("master-948-228c707"), 1, footprint);

    assert_eq!(
        value(&n, "runtime"),
        "stable-diffusion.cpp master-948-228c707"
    );
    assert_eq!(value(&n, "family"), "Flux.1");
    assert_eq!(value(&n, "vae"), "ae.safetensors");
    assert_eq!(value(&n, "clip_l"), "clip_l.safetensors");
    assert_eq!(value(&n, "t5xxl"), "t5xxl_fp16.safetensors");
    assert_eq!(value(&n, "slot"), "secondary");
    assert_eq!(
        value(&n, "memory"),
        "30.0 GiB = files 23.0 GiB + margin 7.0 GiB"
    );
    let labels: Vec<&str> = n.decisions.iter().map(|d| d.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "runtime", "family", "vae", "clip_l", "t5xxl", "slot", "memory"
        ],
        "components in role order"
    );
    assert!(n.decision("ctx").is_none(), "an image model has no context");
}

#[test]
fn an_sd_launch_without_a_record_still_names_the_project_and_the_primary() {
    let footprint = SlotFootprint {
        weights_bytes: 30 * GIB,
        kv_bytes: 0,
    };
    let n = narrate_sd_with(&flux(), None, PRIMARY_SLOT, footprint);

    assert_eq!(value(&n, "runtime"), "stable-diffusion.cpp");
    assert_eq!(
        n.decision("runtime")
            .and_then(|d| d.source.clone())
            .as_deref(),
        Some("no install record")
    );
    assert_eq!(value(&n, "slot"), "primary");
}
