//! A count a repository's size can hold, and the count a file's name states.

use super::*;

#[test]
fn a_count_the_weights_could_not_hold_is_not_the_models() {
    // `HauhauCS`: the Hub's count is its `FastMTP` sidecar's; its name's holds.
    assert!(!plausible_total(1.863_907_84, 31_457_990_784));
    assert!(plausible_total(27.0, 31_457_990_784));
    // unsloth's gemma-3-4b-it: 16 bits a weight at BF16.
    assert!(plausible_total(3.880_263_168, 7_767_803_520));
    // A small model at F32, its metadata counted.
    assert!(plausible_total(0.5, 2_100_000_000));
    // A count far over what the file holds: under a bit a weight.
    assert!(!plausible_total(70.0, 2_000_000_000));
    assert!(!plausible_total(0.0, 1));
}

#[test]
fn a_name_states_its_count_by_its_first_size_token() {
    let named = |name| named_parameters_b(name);
    assert_eq!(
        named("Qwen3.8-27B-Uncensored-HauhauCS-Aggressive-Q8_K_P.gguf"),
        Some(27.0)
    );
    assert_eq!(named("Qwopus3.8-27B-Flash-V2-MTP-Q4_K_M.gguf"), Some(27.0));
    assert_eq!(named("Qwen3-30B-A3B-Q4_K_M.gguf"), Some(30.0));
    assert_eq!(named("Llama-3.1-8B-Instruct.Q4_K_M.gguf"), Some(8.0));
    assert_eq!(named("Phi-3.5-mini-instruct_3.8b-Q4_0.gguf"), Some(3.8));
    assert_eq!(named("gemma-3-270m-it-Q8_0.gguf"), Some(0.27));
    assert_eq!(named("BF16/DeepSeek-V3.1-BF16-00001-of-00030.gguf"), None);
}

#[test]
fn a_name_that_states_no_total_gives_none() {
    let named = |name| named_parameters_b(name);
    // A date and a precision are no size.
    assert_eq!(named("qwen-image-edit-2511-BF16.gguf"), None);
    // A mixture of experts' label states each expert, not the total.
    assert_eq!(named("Mixtral-8x7B-Instruct-v0.1.Q4_K_M.gguf"), None);
    // Only an active count, or a letter before the number.
    assert_eq!(named("gemma-3n-E4B-it-Q4_K_M.gguf"), None);
    assert_eq!(named("model-Q4_K_M.gguf"), None);
    assert_eq!(named("mmproj-F16.gguf"), None);
}

/// A name that agrees with the Hub gives way to it; one that contradicts
/// it is the count, while the largest weights could hold it.
#[test]
fn a_contradicting_name_is_the_count_the_weights_hold() {
    use crate::ports::huggingface::variants_of;

    let gemma = variants_of([
        ("gemma-3-4b-it-BF16.gguf", 7_767_803_520),
        ("mmproj-F16.gguf", 851_251_104),
    ]);
    assert!(!names_contradict(Some(3.880_263_168), &gemma));
    assert_eq!(
        sized_parameters_b(Some(3.880_263_168), &gemma),
        Some(3.880_263_168)
    );

    let qwopus = variants_of([
        ("Qwopus3.8-27B-Flash-V2-MTP-BF16.gguf", 54_657_733_568),
        ("Qwopus3.8-27B-Flash-V2-MTP-Q4_K_M.gguf", 16_810_714_048),
        ("mmproj-F32.gguf", 1_842_940_992),
    ]);
    assert!(names_contradict(Some(0.460_730_096), &qwopus));
    assert!(names_contradict(None, &qwopus));
    assert_eq!(sized_parameters_b(Some(0.460_730_096), &qwopus), Some(27.0));

    // A name the weights could not hold gives way to a Hub count they can.
    let small = variants_of([("model-70B-Q4_K_M.gguf", 2_000_000_000)]);
    assert_eq!(sized_parameters_b(Some(3.0), &small), Some(3.0));
    assert_eq!(sized_parameters_b(None, &small), None);

    // Weights of no known size judge nothing.
    let sizeless = variants_of([("model-70B-Q4_K_M.gguf", 0)]);
    assert_eq!(sized_parameters_b(Some(3.0), &sizeless), Some(3.0));
}
