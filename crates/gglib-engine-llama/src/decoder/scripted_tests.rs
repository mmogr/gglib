//! The scripted decoder holding to the [`Decoder`] contract where the real
//! one must: an empty prompt refused before anything is read or recorded.

use std::path::Path;

use gglib_core::cache_config::KvCacheType;

use super::{Script, ScriptedDecoder};
use crate::decoder::{Abort, Decoder, DecoderError, LoadParams, Sampling, Token};

#[test]
fn an_empty_prompt_is_refused_and_the_cached_prefix_kept() {
    let (mut decoder, record) = ScriptedDecoder::new(Script::new(["Hi"]));
    let params = LoadParams {
        n_ctx: 256,
        cache_type_k: KvCacheType::Q8_0,
        cache_type_v: KvCacheType::Q8_0,
    };
    decoder
        .load(
            Path::new("/models/tiny.gguf"),
            &params,
            &Abort::new(),
            &mut |_| {},
        )
        .expect("the scripted model loads");
    let prompt = decoder.tokenize("Once").expect("the prompt tokenizes");
    let n_prompt = u32::try_from(prompt.len()).unwrap();
    let generate = |decoder: &mut ScriptedDecoder, prompt: &[Token]| {
        decoder.generate(prompt, &Sampling::default(), &Abort::new(), &mut |_| {})
    };
    generate(&mut decoder, &prompt).expect("the first reply runs");

    let refused = generate(&mut decoder, &[]);

    assert_eq!(refused, Err(DecoderError::EmptyPrompt));
    let again = generate(&mut decoder, &prompt).expect("the next reply runs");
    assert_eq!(again.cached_tokens, n_prompt - 1, "the KV cache kept it");
    let prompts = record.lock().unwrap().prompts.len();
    assert_eq!(prompts, 2, "the empty prompt was not recorded");
}
