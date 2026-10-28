# decoder

<!-- module-docs:start -->

llama.cpp behind a seam. [`Decoder`] is what the engine asks of a model:
load one, reporting the fraction read as it goes and stopping at the next
report once its [`Abort`] is raised ([`Loaded`] says what it was trained to,
the context made for it, its chat template and its special tokens), turn
text into tokens, sample a reply as a series of [`Step`]s (how far the prompt
has been read, then each token's text) ending in [`Finished`] counts, and
free it. Every call blocks; the engine runs a generation on a thread of its
own.

A generation ends at an end-of-generation token, at `n_predict` tokens, when
the context is full, or when its [`Abort`] is raised. A decoder checks the
abort before every decode step, so whoever raises it (a client that went
away, a stop string, a model being swapped) ends the generation within one
step. The prompt's leading tokens the previous generation left in the KV cache
are kept rather than read again, which `cached_tokens` reports, as
llama-server's `cache_prompt` does by default. An empty prompt is refused
before the KV cache is touched, since llama.cpp has no logits to sample from
until a token is read; the engine answers one itself, as llama-server does.

[`Sampling`] is the sampler chain's settings, read from a chat request as
llama-server reads them: a key that is absent or of the wrong type keeps
llama-server's own default at the pin, which is what [`Sampling::default`]
holds, so a request gglib's proxy leaves a key out of samples as it would on
the desktop (ADR 0003).

`LlamaDecoder` is the decoder over llama.h, compiled in only when build.rs
linked llama.framework (the `llama` feature, on an Apple target, with
`GGLIB_LLAMA_XCFRAMEWORK` set). It reaches llama.cpp through `crate::sys`
and holds no unsafe code itself. It reads a prompt 512 tokens a step after
the prefix the KV cache keeps (all of it, when the cache cannot be cut or a
sliding window no longer holds what precedes the cut), builds llama-server's
sampler chain for each generation and tells it the prompt first, decodes a
sampled token only when the next is wanted, and hands a token's bytes out as
text once the characters in them are whole. `ScriptedDecoder` (feature
`test-decoder`) replays a `Script` of pieces for tests, its load in a number
of reported steps, tokenizing a character to a token, and keeps a
`ScriptRecord` of what it was asked.

# Structure

- `mod.rs` - the [`Decoder`] trait and what crosses it: [`LoadParams`],
  [`Loaded`], [`Step`], [`PromptProgress`], [`Finished`], [`FinishReason`],
  [`DecoderError`]
- `abort.rs` - [`Abort`], the flag a generation or a load is ended with
- `sampling.rs` - [`Sampling`], its llama-server defaults and its reading of a
  request
- `sampling_tests.rs` - the defaults against the pin's `/props`, and every
  key gglib's proxy sends
- `scripted.rs` - `ScriptedDecoder`, for tests
- `scripted_tests.rs` - `ScriptedDecoder` refusing an empty prompt as
  `LlamaDecoder` does
- `llama.rs` - `LlamaDecoder`: a load, a prompt read after the cached
  prefix, and a reply sampled
- `llama_tests.rs` - the decoder on stories260K and tinygemma3: the trained
  context, the template and special tokens as llama-server reads them, a
  greedy reply and its counts, the cached prefix, an empty prompt refused,
  an abort mid-reply and inside llama.cpp, a load stopped from its progress
  leaving no handle, a context refused, and a real reply streamed by the
  shim
- `utf8.rs` - a token's bytes turned into text as its characters complete

<!-- module-docs:end -->
