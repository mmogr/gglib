//! What each shape of machine is suggested, and the budget its downloads
//! are judged against: the load's.

use gglib_core::domain::recommendation::BudgetSource;
use gglib_core::utils::system::SystemMemoryInfo;

use super::*;

const GIB: u64 = 1024 * 1024 * 1024;

/// A machine with no GPU and `total_ram_bytes` of RAM.
const fn no_gpu(total_ram_bytes: u64) -> SystemMemoryInfo {
    SystemMemoryInfo {
        total_ram_bytes,
        gpu_memory_bytes: None,
        is_unified_memory: false,
        has_nvidia_gpu: false,
    }
}

/// A machine with a card of `vram` and 32 GiB of RAM.
const fn card(vram: u64) -> SystemMemoryInfo {
    SystemMemoryInfo {
        total_ram_bytes: 32 * GIB,
        gpu_memory_bytes: Some(vram),
        is_unified_memory: false,
        has_nvidia_gpu: true,
    }
}

/// With no GPU, a download is judged against the share of RAM the load
/// spends, never the whole of it; with a card, against the card.
#[test]
fn a_download_is_judged_against_the_loads_budget() {
    let ram = 79 * GIB / 10;
    assert_eq!(
        download_budget_on(false, &no_gpu(ram)),
        super::super::fit_memory_of(false, &no_gpu(ram)).budget_bytes()
    );
    assert!(download_budget_on(false, &no_gpu(ram)) < Some(ram));
    assert_eq!(download_budget_on(true, &card(8 * GIB)), Some(8 * GIB));
}

/// A GPU whose memory could not be read gives the load no budget, so a
/// download has none either, and nothing is suggested: neither sizes that
/// card against the machine's RAM.
#[test]
fn a_gpu_whose_memory_could_not_be_read_has_no_budget_and_no_suggestion() {
    let unread = no_gpu(64 * GIB);
    assert_eq!(download_budget_on(true, &unread), None);
    assert!(recommend_on(true, &unread).is_none());
}

/// A Mac (or an integrated GPU) with `total_ram_bytes` of RAM, whose GPU is
/// given the unified share of it.
fn unified(total_ram_bytes: u64) -> SystemMemoryInfo {
    SystemMemoryInfo {
        total_ram_bytes,
        gpu_memory_bytes: Some(crate::system::gpu::unified_share(total_ram_bytes)),
        is_unified_memory: true,
        has_nvidia_gpu: false,
    }
}

/// What a suggestion on a machine comes to: the model, by its size, and
/// the context a load serves it, or nothing.
fn suggested(gpu_present: bool, mem: &SystemMemoryInfo) -> Option<(&'static str, u64)> {
    recommend_on(gpu_present, mem).map(|rec| {
        let size = match rec.candidate.repo {
            "unsloth/Qwen3-30B-A3B-GGUF" => "30B",
            "bartowski/Qwen2.5-Coder-14B-Instruct-GGUF" => "14B",
            "bartowski/Qwen2.5-Coder-7B-Instruct-GGUF" => "7B",
            "bartowski/Qwen2.5-Coder-3B-Instruct-GGUF" => "3B",
            other => panic!("{other}"),
        };
        (size, rec.fit.serves_at)
    })
}

/// What each shape of machine is suggested, by the load's rule, and the
/// context a load serves it. A change here is a product decision.
///
/// The suggestions do not rise steadily with memory, and must not be made
/// to: they follow the load. A 6 GiB card gets the 7B and a 7 GiB card the
/// 3B, because a load gives the 7B the whole 6 GiB card (it does not fit the
/// card less a second slot's room) but only 8K of the 7 GiB card (it fits
/// that less the room, at 8K, and is served there); so, with no GPU, 8 GiB
/// of RAM gets the 7B and 9 GiB the 3B. Where no row reaches
/// its 32K, the row served the most context from 8K is suggested (a 6 GiB
/// PC: the 3B at 16K).
#[test]
fn each_machine_is_suggested_what_its_load_serves() {
    let gib = |tenths: u64| tenths * GIB / 10;
    let no_gpus = [
        (40, Some(("3B", 32_768))),
        (60, Some(("3B", 16_384))),
        (79, Some(("7B", 32_768))),
        (80, Some(("7B", 32_768))),
        (90, Some(("3B", 32_768))),
        (160, Some(("7B", 32_768))),
        (240, Some(("14B", 32_768))),
        (320, Some(("30B", 32_768))),
    ];
    let cards = [
        (60, Some(("7B", 32_768))),
        (70, Some(("3B", 32_768))),
        (80, Some(("7B", 32_768))),
        (120, Some(("7B", 32_768))),
        (160, Some(("14B", 32_768))),
        (240, Some(("30B", 32_768))),
    ];
    let macs = [
        (160, Some(("7B", 32_768))),
        (180, Some(("7B", 32_768))),
        (320, Some(("30B", 32_768))),
        (640, Some(("30B", 32_768))),
    ];
    let got = |rows: &[(u64, Option<(&'static str, u64)>)],
               at: &dyn Fn(u64) -> Option<(&'static str, u64)>| {
        rows.iter()
            .map(|&(tenths, _)| (tenths, at(gib(tenths))))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        got(&no_gpus, &|ram| suggested(false, &no_gpu(ram))),
        no_gpus
    );
    assert_eq!(got(&cards, &|vram| suggested(true, &card(vram))), cards);
    assert_eq!(got(&macs, &|ram| suggested(true, &unified(ram))), macs);
}

/// With no GPU the suggestion says the machine's RAM, and that the model is
/// sized to it, never a card.
#[test]
fn a_machine_with_no_gpu_is_told_its_ram() {
    for ram in [6 * GIB, 16 * GIB] {
        let rec = recommend_on(false, &no_gpu(ram)).expect("suggested");
        assert_eq!(rec.budget_source, BudgetSource::SystemRam);
        assert_eq!(rec.fit.system_ram_bytes, Some(ram));
    }
}

/// A 6 GiB PC, where no row reaches its 32K, is suggested the 3B at the
/// 16K a load serves it, and told so in the load's own words.
#[test]
fn a_6_gib_pc_is_told_the_context_it_gets() {
    let rec = recommend_on(false, &no_gpu(6 * GIB)).expect("suggested");
    assert_eq!(rec.candidate.quantization, Some("Q4_K_M"));
    assert_eq!(
        rec.fit.sentence,
        "Fits this machine's 6.0 GiB of RAM (no GPU) with 16K context, of 32K trained."
    );
}

/// The VM of #2062 is told, in the load's own words, what a load of the
/// suggested size will say once it is down, and nothing of a card.
#[test]
fn the_vm_is_told_the_loads_sentence() {
    let rec = recommend_on(false, &no_gpu(79 * GIB / 10)).expect("suggested");
    assert_eq!(rec.candidate.quantization, Some("Q4_K_M"));
    assert_eq!(
        rec.fit.sentence,
        "Fits this machine's 7.9 GiB of RAM (no GPU) with 32K context, of 32K trained."
    );
    assert_eq!(
        rec.fit.note.as_deref(),
        Some("fits (5.29 GiB of 5.33 GiB usable)")
    );
}

/// A card is sized against its own memory, as before.
#[test]
fn a_card_is_suggested_by_its_memory() {
    let rec = recommend_on(true, &card(8 * GIB)).expect("suggested");
    assert_eq!(rec.budget_source, BudgetSource::Vram);
    assert_eq!(rec.budget_bytes, 8 * GIB);
    assert_eq!(
        rec.candidate.repo,
        "bartowski/Qwen2.5-Coder-7B-Instruct-GGUF"
    );
}
