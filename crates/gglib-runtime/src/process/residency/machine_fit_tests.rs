//! The context a launch is sized to, as a pure function of what was
//! detected: whether a GPU is present, the memory read of it, the machine's
//! RAM, the model, and the user's `default-context-size`.
//!
//! Each case goes through `prepare_with`, the one place a launch and a plan
//! of one resolve their context, with the machine passed in as values
//! ([`fit_memory_of`]), so every OS runs every case.

use gglib_core::cache_config::CacheRamSetting;
use gglib_core::domain::{KvElemsPerToken, KvShape};
use gglib_core::ports::{LaunchOverrides, ModelLaunchSpec};
use gglib_core::server_config::{ContextSizeSource, ServerConfigOptions};
use gglib_core::settings::DEFAULT_CONTEXT_SIZE;
use gglib_core::utils::system::SystemMemoryInfo;

use super::launch::LaunchRequest;
use super::prepare_with;
use super::residency_tests::launch_spec;
use crate::system::fit_memory_of;

const GIB: u64 = 1024 * 1024 * 1024;

/// The Windows 11 VM of issue #1959: 7.9 GiB of RAM, no GPU (Microsoft's
/// Basic Display Adapter only).
const VM_RAM: u64 = 79 * GIB / 10;

/// What was detected of one machine.
#[derive(Clone, Copy)]
struct Machine {
    gpu_present: bool,
    gpu_memory_bytes: Option<u64>,
    is_unified_memory: bool,
    total_ram_bytes: u64,
}

/// A machine with no GPU and `total_ram_bytes` of RAM.
const fn no_gpu(total_ram_bytes: u64) -> Machine {
    Machine {
        gpu_present: false,
        gpu_memory_bytes: None,
        is_unified_memory: false,
        total_ram_bytes,
    }
}

/// Qwen3-4B at `Q4_K_M`, the model loaded on the VM: 2.33 GiB of weights,
/// 36 layers of 8 KV heads of 128, trained for 40,960 tokens.
fn qwen3_4b() -> ModelLaunchSpec {
    let mut spec = launch_spec(41, "qwen3-4b");
    spec.file_size_bytes = 2_500_000_000;
    spec.context_length = Some(40_960);
    spec.kv_shape = Some(KvShape::uniform(KvElemsPerToken {
        k: 36 * 8 * 128,
        v: 36 * 8 * 128,
    }));
    spec
}

/// The launch `spec` resolves to on `machine`, with the user's
/// `default-context-size` and a context asked for by the request.
fn launch_on(
    machine: Machine,
    spec: ModelLaunchSpec,
    default_ctx: Option<u64>,
    num_ctx: Option<u64>,
) -> LaunchRequest {
    let memory = fit_memory_of(
        machine.gpu_present,
        &SystemMemoryInfo {
            total_ram_bytes: machine.total_ram_bytes,
            gpu_memory_bytes: machine.gpu_memory_bytes,
            is_unified_memory: machine.is_unified_memory,
            has_nvidia_gpu: false,
        },
    );
    prepare_with(
        Some(memory),
        spec,
        &ServerConfigOptions::default(),
        CacheRamSetting::Auto,
        num_ctx,
        default_ctx,
        &LaunchOverrides::default(),
    )
}

fn sentence(request: &LaunchRequest) -> &str {
    &request
        .fit
        .as_ref()
        .expect("a chat model has a fit")
        .sentence
}

/// The VM's case. Sized as a unified-memory GPU is: three quarters of RAM
/// (5.93 GiB), less a top-up for a second slot (leaving 4.11 GiB), of which the
/// fit spends 90% (3.70 GiB). The weights take 2.33 GiB, leaving 1.37 GiB of
/// `q8_0` KV cache at 78,336 bytes a token: 18,839 tokens, snapped to 16K.
#[test]
fn with_no_gpu_the_context_is_sized_against_system_ram() {
    let request = launch_on(no_gpu(VM_RAM), qwen3_4b(), None, None);
    assert_eq!(
        request.context,
        (16_384, ContextSizeSource::FittedToHardware)
    );
    let fit = request.fit.as_ref().expect("a chat model has a fit");
    assert_eq!(fit.unsnapped, Some(18_839));
    assert_eq!(fit.system_ram_bytes, Some(VM_RAM));
}

/// The user is told the truth in one line: what context, and that it is
/// sized to this machine's RAM because there is no GPU. Never that the
/// machine's memory could not be read, nor anything of VRAM.
#[test]
fn with_no_gpu_the_sentence_says_the_context_is_sized_to_ram() {
    let request = launch_on(no_gpu(VM_RAM), qwen3_4b(), None, None);
    assert_eq!(
        sentence(&request),
        "Fits this machine's 7.9 GiB of RAM (no GPU) with 16K context, of 40K trained."
    );
}

/// Plenty of RAM never claims more than the model was trained for.
#[test]
fn with_no_gpu_the_context_is_never_above_the_trained_window() {
    let request = launch_on(no_gpu(256 * GIB), qwen3_4b(), None, None);
    assert_eq!(
        request.context,
        (32_768, ContextSizeSource::FittedToHardware),
        "the largest rung inside 40,960 trained"
    );
}

/// A model RAM cannot hold is never fitted below the floor: the launch
/// serves the built-in floor, and the sentence says it could not fit one to
/// this machine's RAM, not that the RAM could not be read.
#[test]
fn with_no_gpu_a_model_too_big_for_ram_gets_the_floor_and_says_why() {
    let mut big = qwen3_4b();
    big.file_size_bytes = 6 * GIB;
    let request = launch_on(no_gpu(VM_RAM), big, None, None);
    assert_eq!(
        request.context,
        (DEFAULT_CONTEXT_SIZE, ContextSizeSource::BuiltInDefault)
    );
    assert_eq!(
        sentence(&request),
        "Serves at 4K context, of 40K trained; gglib could not fit one to this machine's 7.9 GiB of RAM (no GPU)."
    );
}

/// The user's `default-context-size` outranks the fit, GPU or none.
#[test]
fn with_no_gpu_the_users_default_context_size_still_wins() {
    let request = launch_on(no_gpu(VM_RAM), qwen3_4b(), Some(8192), None);
    assert_eq!(request.context, (8192, ContextSizeSource::GlobalDefault));
}

/// A larger context asked for is served as asked, and the sentence says
/// honestly that it is more than fits, and what does.
#[test]
fn with_no_gpu_a_larger_context_asked_for_says_what_fits() {
    let request = launch_on(no_gpu(VM_RAM), qwen3_4b(), None, Some(32_768));
    assert_eq!(request.context, (32_768, ContextSizeSource::Explicit));
    assert_eq!(
        sentence(&request),
        "Serves at 32K context, of 40K trained; 16K fits this machine's 7.9 GiB of RAM (no GPU)."
    );
    let note = request.fit.as_ref().and_then(|f| f.note.as_deref());
    assert!(
        note.is_some_and(|n| n.starts_with("does not fit")),
        "{note:?}"
    );
}

/// D4: a GPU that exists but whose memory cannot be read keeps today's
/// refusal. The floor, and the sentence that says why.
#[test]
fn a_gpu_whose_memory_cannot_be_read_keeps_the_refusal() {
    let unreadable = Machine {
        gpu_present: true,
        ..no_gpu(64 * GIB)
    };
    let request = launch_on(unreadable, qwen3_4b(), None, None);
    assert_eq!(
        request.context,
        (DEFAULT_CONTEXT_SIZE, ContextSizeSource::BuiltInDefault)
    );
    assert_eq!(
        sentence(&request),
        "Serves at 4K context, of 40K trained; gglib could not fit one to this machine: this machine's memory could not be read."
    );
    assert_eq!(request.fit.as_ref().and_then(|f| f.budget_bytes), None);
}

/// A GPU whose memory is read is sized against it, as before.
#[test]
fn a_readable_gpu_is_sized_against_its_memory() {
    let card = Machine {
        gpu_present: true,
        gpu_memory_bytes: Some(24 * GIB),
        ..no_gpu(64 * GIB)
    };
    let request = launch_on(card, qwen3_4b(), None, None);
    assert_eq!(
        request.context,
        (32_768, ContextSizeSource::FittedToHardware)
    );
    assert_eq!(
        sentence(&request),
        "Fits this machine with 32K context, of 40K trained."
    );
    assert_eq!(request.fit.as_ref().and_then(|f| f.system_ram_bytes), None);
}

/// Apple Silicon's unified memory is a GPU's reading, as before: an 8 GiB
/// Mac is sized against its 6 GiB share and says nothing of RAM.
#[test]
fn unified_memory_is_sized_as_a_gpu() {
    let mac = Machine {
        gpu_present: true,
        gpu_memory_bytes: Some(6 * GIB),
        is_unified_memory: true,
        total_ram_bytes: 8 * GIB,
    };
    let request = launch_on(mac, qwen3_4b(), None, None);
    assert_eq!(
        request.context,
        (16_384, ContextSizeSource::FittedToHardware)
    );
    assert_eq!(
        sentence(&request),
        "Fits this machine with 16K context, of 40K trained."
    );
}
