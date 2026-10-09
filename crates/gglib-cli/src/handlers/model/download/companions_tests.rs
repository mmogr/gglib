//! The companions `gglib model download` names before it queues an image
//! model: from a Hub's listing and the head of its weights, through the
//! listing the web page's preview shows, to the lines printed.

use std::path::Path;

use gglib_app_services::types::HfCompanion;
use gglib_core::domain::{ComponentRole, ImageFamily};
use gglib_core::paths::repository_dir;
use gglib_core::ports::HfClientPort;
use gglib_core::ports::huggingface::fake_hub::{FakeHub, hub_file};

use super::*;
use crate::bootstrap::test_context;
use crate::handlers::model::test_library::FLUX_TENSORS;

const REPO: &str = "leejet/FLUX.1-schnell-gguf";
const WEIGHTS: &str = "flux1-schnell-q8_0.gguf";

/// The bytes a GGUF of `tensors` and `pairs` begins with, as written by the
/// fixture writer the parser's own tests read.
fn head(dir: &Path, pairs: &[(&str, &str)], tensors: &[(&str, &[u64])]) -> Vec<u8> {
    let path = dir.join("head.gguf");
    gglib_gguf::write_tensor_gguf(&path, pairs, tensors);
    std::fs::read(path).unwrap()
}

/// A Hub whose one quantization's file begins with `head`, and which lists
/// every companion of the Flux recipe at the recipe's size.
fn hub(head: Vec<u8>) -> FakeHub {
    FakeHub {
        weights: vec![hub_file(WEIGHTS, 12_000_000_000, "w")],
        heads: vec![(WEIGHTS.to_owned(), head)],
        files_at: ImageFamily::Flux1
            .recipe()
            .components
            .iter()
            .map(|spec| (spec.repo.to_owned(), hub_file(spec.path, spec.size, "c")))
            .collect(),
        ..FakeHub::default()
    }
}

/// The listing's operations over a library in `dir`, asking `hub`, and
/// looking for companions under `models`.
async fn ops(dir: &Path, hub: FakeHub, models: &Path) -> DownloadOps {
    gglib_core::paths::isolate_data_root();
    let mut ctx = test_context(dir).await;
    ctx.hf_client = Arc::new(hub) as Arc<dyn HfClientPort>;
    listing_ops(&ctx, Some(models.to_path_buf()))
}

fn size_of(role: ComponentRole) -> u64 {
    let recipe = ImageFamily::Flux1.recipe();
    let spec = recipe.components.iter().find(|spec| spec.role == role);
    spec.unwrap().size
}

/// A Flux head names the family and its three companions, the one already
/// in its repository's folder marked so, and the bytes of the other two to
/// fetch.
#[tokio::test]
async fn a_flux_repository_names_its_companions_and_what_is_fetched() {
    let dir = tempfile::tempdir().unwrap();
    let models = tempfile::tempdir().unwrap();
    let vae_dir = repository_dir(models.path(), "unsloth/FLUX.1-schnell");
    std::fs::create_dir_all(&vae_dir).unwrap();
    std::fs::write(vae_dir.join("ae.safetensors"), b"here").unwrap();
    let ops = ops(
        dir.path(),
        hub(head(dir.path(), &[], FLUX_TENSORS)),
        models.path(),
    )
    .await;

    let text = preview(&ops, REPO)
        .await
        .expect("a Flux repository's preview");

    let fetch = size_of(ComponentRole::ClipL) + size_of(ComponentRole::T5xxl);
    assert_eq!(
        text,
        format!(
            "Flux.1 image model: its download brings 3 companion file(s) beside the weights.
  VAE     unsloth/FLUX.1-schnell/ae.safetensors ({}, already here)
  CLIP-L  comfyanonymous/flux_text_encoders/clip_l.safetensors ({})
  T5-XXL  comfyanonymous/flux_text_encoders/t5xxl_fp16.safetensors ({})
  To fetch beside the weights: {}
",
            format_size(size_of(ComponentRole::Vae)),
            format_size(size_of(ComponentRole::ClipL)),
            format_size(size_of(ComponentRole::T5xxl)),
            format_size(fetch),
        )
    );
}

/// A repository whose weights are a chat model's says nothing.
#[tokio::test]
async fn a_chat_repository_has_no_preview() {
    let dir = tempfile::tempdir().unwrap();
    let models = tempfile::tempdir().unwrap();
    let chat = head(dir.path(), &[("general.architecture", "qwen3")], &[]);
    let ops = ops(dir.path(), hub(chat), models.path()).await;

    assert_eq!(preview(&ops, REPO).await, None);
}

/// A listing the Hub cannot answer says nothing either: the download is
/// queued all the same, and fails, if it does, in the daemon's words.
#[tokio::test]
async fn a_listing_that_fails_has_no_preview() {
    let dir = tempfile::tempdir().unwrap();
    let models = tempfile::tempdir().unwrap();
    let ops = ops(dir.path(), FakeHub::default(), models.path()).await;

    assert_eq!(preview(&ops, REPO).await, None);
}

/// When every companion is already here, only the weights are fetched, and
/// the last line says so.
#[test]
fn every_companion_here_fetches_only_the_weights() {
    let companion = |role: ComponentRole, file_path: &str| HfCompanion {
        role,
        repo: "o/r".to_owned(),
        file_path: file_path.to_owned(),
        size_bytes: 2048,
        present: true,
    };
    let all_here = HfImagePreview {
        family: ImageFamily::QwenImage21,
        companions: vec![
            companion(ComponentRole::Vae, "vae.safetensors"),
            companion(ComponentRole::Llm, "llm.gguf"),
        ],
        fetch_bytes: 0,
    };

    assert_eq!(
        preview_text(&all_here),
        "Qwen-Image 2.1 image model: its download brings 2 companion file(s) beside the weights.
  VAE  o/r/vae.safetensors (2.00 KiB, already here)
  LLM  o/r/llm.gguf (2.00 KiB, already here)
  Every companion is already here; only the weights are fetched.
"
    );
}
