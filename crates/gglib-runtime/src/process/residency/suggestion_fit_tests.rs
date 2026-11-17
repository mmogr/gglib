//! Setup's suggestion, the size picker and the load answer "does this fit
//! this machine?" by one rule (#2062): a size the suggestion downloads, or
//! the picker calls fitting, is one the load fits, at the context the picker
//! names.
//!
//! The load is `prepare_with`, the one place a launch resolves its context,
//! told the machine as values ([`fit_memory_of`]); the suggestion and the
//! picker's budget are runtime's own (`recommend_on`, `download_budget_on`);
//! the picker's verdict and pick are core's (`MemoryFit`, `pick_variant`).

use std::collections::HashMap;

use async_trait::async_trait;
use gglib_core::cache_config::CacheRamSetting;
use gglib_core::domain::gguf::{GgufMetadata, HeadMetadata};
use gglib_core::domain::{FileType, VariantFit, estimate_kv_shape};
use gglib_core::download::{
    Contender, FitJudge, GgufFileRole, HeadFacts, MemoryFit, VariantReader, pick_variant,
};
use gglib_core::ports::LaunchOverrides;
use gglib_core::server_config::ServerConfigOptions;
use gglib_core::utils::system::SystemMemoryInfo;

use super::prepare_with;
use super::residency_tests::launch_spec;
use crate::system::{download_budget_on, fit_memory_of, recommend_on};

const GIB: u64 = 1024 * 1024 * 1024;

/// The repository setup suggested on the VM of #2062.
const REPO: &str = "bartowski/Qwen2.5-Coder-7B-Instruct-GGUF";

/// Its weights, every variant, exact bytes from the Hugging Face API at
/// revision `1f629da0c8bed16b9e50cee91c70693650e66c35`.
const SIZES: &[(&str, u64)] = &[
    ("IQ2_M", 2_780_343_072),
    ("Q2_K", 3_015_940_896),
    ("IQ3_XS", 3_346_256_672),
    ("Q3_K_S", 3_492_369_184),
    ("Q2_K_L", 3_548_164_896),
    ("IQ3_M", 3_574_012_704),
    ("Q3_K_M", 3_808_391_968),
    ("Q3_K_L", 4_088_460_064),
    ("IQ4_XS", 4_218_473_248),
    ("Q4_0_4_4", 4_431_391_520),
    ("Q4_0_4_8", 4_431_391_520),
    ("Q4_0_8_8", 4_431_391_520),
    ("Q4_0", 4_444_121_888),
    ("Q4_K_S", 4_457_769_760),
    ("Q3_K_XL", 4_565_332_768),
    ("Q4_K_M", 4_683_074_336),
    ("Q4_K_L", 5_087_564_576),
    ("Q5_K_S", 5_315_177_248),
    ("Q5_K_M", 5_444_832_032),
    ("Q5_K_L", 5_781_197_600),
    ("Q6_K", 6_254_199_584),
    ("Q6_K_L", 6_518_182_688),
    ("Q8_0", 8_098_525_984),
    ("f16", 15_237_853_696),
];

/// What was detected of one machine.
struct Machine {
    name: &'static str,
    gpu_present: bool,
    memory: SystemMemoryInfo,
}

/// The Windows 11 VM of #2062: 7.9 GiB of RAM, no GPU.
fn the_vm() -> Machine {
    Machine {
        name: "7.9 GiB of RAM, no GPU",
        gpu_present: false,
        memory: SystemMemoryInfo {
            total_ram_bytes: 79 * GIB / 10,
            gpu_memory_bytes: None,
            is_unified_memory: false,
            has_nvidia_gpu: false,
        },
    }
}

/// A PC with an 8 GiB card and 32 GiB of RAM.
fn an_8_gib_card() -> Machine {
    Machine {
        name: "an 8 GiB card",
        gpu_present: true,
        memory: SystemMemoryInfo {
            total_ram_bytes: 32 * GIB,
            gpu_memory_bytes: Some(8 * GIB),
            is_unified_memory: false,
            has_nvidia_gpu: true,
        },
    }
}

/// The header every variant shares, as its first bytes give it: Qwen2,
/// 28 layers of 4 KV heads of 128, trained for 32K.
fn header(file_type: FileType) -> GgufMetadata {
    let metadata: HashMap<String, String> = [
        ("general.architecture", "qwen2"),
        ("qwen2.context_length", "32768"),
        ("qwen2.block_count", "28"),
        ("qwen2.embedding_length", "3584"),
        ("qwen2.attention.head_count", "28"),
        ("qwen2.attention.head_count_kv", "4"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect();
    GgufMetadata {
        architecture: Some("qwen2".to_owned()),
        file_type: Some(file_type),
        context_length: Some(32_768),
        metadata,
        role: GgufFileRole::Weights,
        ..Default::default()
    }
}

/// A variant's type as its header states it: `F16` for `f16`, and a
/// quantized type for the rest.
fn file_type(label: &str) -> FileType {
    if label == "f16" {
        FileType(1)
    } else {
        FileType(15)
    }
}

/// The repository's variants, each read whole from its head.
struct Heads;

#[async_trait]
impl VariantReader for Heads {
    async fn head(&self, index: usize) -> Option<HeadFacts> {
        Some(HeadFacts {
            head: HeadMetadata {
                metadata: header(file_type(SIZES[index].0)),
                complete: true,
                role: Some(GgufFileRole::Weights),
                ..Default::default()
            },
            image: false,
        })
    }

    async fn header(&self, index: usize) -> Option<GgufMetadata> {
        Some(header(file_type(SIZES[index].0)))
    }
}

/// The context the load serves `weights_bytes` of this model at on
/// `machine`, when it fits there: its fit reached a rung and the served
/// context's memory is inside the share it spends. `None` when it does not.
fn load_fits(machine: &Machine, weights_bytes: u64) -> Option<u64> {
    let head = header(FileType(15));
    let mut spec = launch_spec(7, "qwen2.5-coder-7b");
    spec.file_size_bytes = weights_bytes;
    spec.context_length = head.context_length;
    spec.kv_shape = estimate_kv_shape(&head.metadata, head.architecture.as_deref());
    let request = prepare_with(
        Some(fit_memory_of(machine.gpu_present, &machine.memory)),
        spec,
        &ServerConfigOptions::default(),
        CacheRamSetting::Auto,
        None,
        None,
        &LaunchOverrides::default(),
    );
    let fit = request.fit.expect("a chat model has a fit");
    let fits = fit.fitted.is_some() && fit.note.as_deref().is_some_and(|n| n.starts_with("fits"));
    fits.then_some(request.context.0)
}

/// The picker's judge on `machine`.
fn picker(machine: &Machine) -> MemoryFit {
    MemoryFit {
        budget_bytes: download_budget_on(machine.gpu_present, &machine.memory)
            .expect("the machine's memory was read"),
    }
}

/// The variant a download of the repository that names none takes on
/// `machine`.
async fn picked(machine: &Machine) -> &'static str {
    let contenders: Vec<Contender> = SIZES
        .iter()
        .map(|&(_, size)| Contender {
            size,
            role: Some(GgufFileRole::Weights),
        })
        .collect();
    let judge = picker(machine);
    let pick = pick_variant(&contenders, &Heads, Some(&judge as &dyn FitJudge))
        .await
        .expect("the repository holds weights");
    SIZES[pick.index].0
}

/// The bytes of the variant labelled `label`.
fn size_of(label: &str) -> u64 {
    SIZES
        .iter()
        .find(|&&(l, _)| l == label)
        .map_or_else(|| panic!("no variant {label}"), |&(_, size)| size)
}

/// What "Get it" downloads on `machine`, as the download queue takes it: the
/// variant the suggestion names, or else the download's own pick.
async fn suggested_download(machine: &Machine) -> &'static str {
    let rec = recommend_on(machine.gpu_present, &machine.memory)
        .unwrap_or_else(|| panic!("{}: nothing suggested", machine.name));
    assert_eq!(rec.candidate.repo, REPO, "{}", machine.name);
    match rec.candidate.quantization {
        Some(label) => label,
        None => picked(machine).await,
    }
}

/// The size "Get it" downloads is the size the suggestion's sentence is
/// about (the row's weights), and the load fits it at the context the row
/// was suggested for.
#[tokio::test]
async fn the_suggestion_downloads_the_size_it_speaks_of_and_the_load_fits_it() {
    for machine in [the_vm(), an_8_gib_card()] {
        let rec = recommend_on(machine.gpu_present, &machine.memory).expect("suggested");
        let label = suggested_download(&machine).await;
        assert_eq!(
            size_of(label),
            rec.candidate.weights_bytes,
            "{}: Get it downloads {label}, not the size the sentence is about",
            machine.name
        );
        assert_eq!(
            load_fits(&machine, size_of(label)),
            Some(rec.candidate.context),
            "{}: the load does not fit {label} at the suggested context",
            machine.name
        );
    }
}

/// Every size the picker calls fitting, the load fits, at the context the
/// picker names; every size it calls too big, the load cannot fit. Each
/// disagreement is listed.
#[test]
fn every_size_the_picker_calls_fitting_the_load_fits_at_that_context() {
    let mut disagreements = Vec::new();
    for machine in [the_vm(), an_8_gib_card()] {
        let judge = picker(&machine);
        for &(label, size) in SIZES {
            let (_, verdict) = judge.judge(&header(file_type(label)), false, size);
            let load = load_fits(&machine, size);
            let agrees = match verdict {
                VariantFit::Fits { ctx: Some(ctx) } | VariantFit::Tight { ctx } => {
                    load == Some(ctx)
                }
                VariantFit::TooBig { .. } => load.is_none(),
                other => panic!("{}: {label} {other:?}", machine.name),
            };
            if !agrees {
                disagreements.push(format!(
                    "{}: the picker says {label} {}, the load {}",
                    machine.name,
                    verdict.word(),
                    load.map_or_else(|| "does not fit it".to_owned(), |c| format!("serves {c}"))
                ));
            }
        }
    }
    assert!(disagreements.is_empty(), "\n{}", disagreements.join("\n"));
}

/// What the load serves each size at on a machine with a GPU, pinned, so a
/// change to the suggestion or the picker is shown not to move the GPU's
/// load. Not monotonic in size: a size that fits the card less a second
/// slot's room is served there (`Q5_K_L` at 8K), and one that does not
/// falls back to the whole card (`Q6_K` at 32K).
#[test]
fn the_load_on_an_8_gib_card_serves_each_size_as_it_did() {
    let card = an_8_gib_card();
    let served: Vec<(&str, Option<u64>)> = SIZES
        .iter()
        .map(|&(label, size)| (label, load_fits(&card, size)))
        .collect();
    let mut expected: Vec<(&str, Option<u64>)> = SIZES[..17]
        .iter()
        .map(|&(label, _)| (label, Some(32_768)))
        .collect();
    expected.extend([
        ("Q5_K_S", Some(16_384)),
        ("Q5_K_M", Some(16_384)),
        ("Q5_K_L", Some(8192)),
        ("Q6_K", Some(32_768)),
        ("Q6_K_L", Some(32_768)),
        ("Q8_0", None),
        ("f16", None),
    ]);
    assert_eq!(served, expected);
}

/// The variant a download that names none takes (the picker's ★) is one the
/// load fits.
#[tokio::test]
async fn the_variant_a_download_picks_is_one_the_load_fits() {
    for machine in [the_vm(), an_8_gib_card()] {
        let label = picked(&machine).await;
        assert!(
            load_fits(&machine, size_of(label)).is_some(),
            "{}: a download picks {label}, which the load does not fit",
            machine.name
        );
    }
}
