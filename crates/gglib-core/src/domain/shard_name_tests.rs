use super::*;

#[test]
fn a_shard_splits_into_its_name_and_total() {
    assert_eq!(
        shard_parts("big-Q4_K_M-00002-of-00003"),
        Some(("big-Q4_K_M", "00003"))
    );
    assert_eq!(shard_parts("big-Q4_K_M"), None);
    assert_eq!(
        shard_parts("one-of-us"),
        None,
        "a name holding -of- is no shard"
    );
}

#[test]
fn a_file_goes_by_its_name_less_the_extension_and_the_numbering() {
    assert_eq!(
        gguf_stem("z_image_nsfw_v2-Q8_0.gguf"),
        "z_image_nsfw_v2-Q8_0"
    );
    assert_eq!(gguf_stem("Model-Q8_0.GGUF"), "Model-Q8_0");
    assert_eq!(
        gguf_stem("Next-UD-IQ3_XXS-00001-of-00004.gguf"),
        "Next-UD-IQ3_XXS"
    );
    assert_eq!(gguf_stem("weights.bin"), "weights.bin");
}
