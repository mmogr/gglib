# decoder

<!-- module-docs:start -->

llama.cpp behind a seam. [`Decoder`] is what the engine asks of a model:
load one ([`Loaded`] says what it was trained to, the context made for it,
its chat template and its special tokens), turn text into tokens, sample a
reply as a series of [`Step`]s (how far the prompt has been read, then each
token's text) ending in [`Finished`] counts, and free it. Every call blocks;
the engine runs a generation on a thread of its own.

A generation ends at an end-of-generation token, at `n_predict` tokens, when
the context is full, or when its [`Abort`] is raised. A decoder checks the
abort before every decode step, so whoever raises it (a client that went
away, a stop string, a model being swapped) ends the generation within one
step. The prompt's leading tokens the previous generation left in the KV cache
are kept rather than read again, which `cached_tokens` reports, as
llama-server's `cache_prompt` does by default.

[`Sampling`] is the sampler chain's settings, read from a chat request as
llama-server reads them: a key that is absent or of the wrong type keeps
llama-server's own default at the pin, which is what [`Sampling::default`]
holds, so a request gglib's proxy leaves a key out of samples as it would on
the desktop (ADR 0003).

This module holds no decoder over llama.h and is pure Rust on every
target. `ScriptedDecoder` (feature `test-decoder`) replays a `Script` of
pieces for tests, tokenizing a character to a token, and keeps a
`ScriptRecord` of what it was asked.

# Structure

- `mod.rs` - the [`Decoder`] trait and what crosses it: [`LoadParams`],
  [`Loaded`], [`Step`], [`PromptProgress`], [`Finished`], [`FinishReason`],
  [`DecoderError`]
- `abort.rs` - [`Abort`], the flag a generation is ended with
- `sampling.rs` - [`Sampling`], its llama-server defaults and its reading of a
  request
- `sampling_tests.rs` - the defaults against the pin's `/props`, and every
  key gglib's proxy sends
- `scripted.rs` - `ScriptedDecoder`, for tests

<!-- module-docs:end -->
