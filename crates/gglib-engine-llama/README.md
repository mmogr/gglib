# gglib-engine-llama

<!-- crate-header:start -->
![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-engine-llama-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-engine-llama-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-engine-llama-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-engine-llama-complexity.json)
<!-- crate-header:end -->

llama.cpp in-process, for a device that cannot start llama-server: a phone.
gglib-proxy forwards a chat to an upstream over HTTP and reads what it answers
in the pinned llama-server's shape. This crate is that upstream without the
process: a [`Decoder`] holds the model, an [`Engine`] runs one generation at
a time on it, and the shim, an axum server on a free port of 127.0.0.1,
answers the proxy's requests as llama-server answers them at the pin.
[`InProcessRuntime`] is the model runtime port the proxy admits each chat
through: it fits one model at a time to the device's memory, loads it, and
hands the proxy the shim as the model's server.

The crate is pure Rust on every target unless its `llama` feature links
llama.cpp. [`Decoder`] is the seam a decoder plugs into. `LlamaDecoder` is the
real one: llama.cpp's own `llama.framework` at the pin, reached through a
small C shim (`csrc/`) whose arguments are scalars, strings and opaque
handles, so no llama.h struct crosses into Rust. build.rs compiles the shim
and links the framework only for an Apple target with the `llama` feature on
and `GGLIB_LLAMA_XCFRAMEWORK` naming the framework `make phone-llama` builds;
anywhere else the feature compiles nothing and `LlamaDecoder` does not exist.
[`linked_llama_decoder`] is how a crate composing the engine asks for it
without naming it: a `LlamaDecoder` where it was linked, and `None` anywhere
else, for a crate that turns the feature on and cannot tell at compile time
whether it linked.
`ScriptedDecoder` (feature `test-decoder`) replays a script of pieces, so this
crate's tests, and the tests of a crate composing it, run on any OS with no
model.

## Architecture

An infrastructure crate and a phone crate: it depends on `gglib-core` (the
pin, the request a chat is sent with, and the runtime port, catalog port,
context fit and device budget the runtime is built on) and `gglib-gguf` (a
chat rendered through its template as llama-server renders it), and on axum
and tokio. It does not depend on gglib-proxy, which is its client, nor on
gglib-runtime or sysinfo, which a phone build cannot take
(`scripts/check_boundaries.sh`). The tests of gglib-proxy served in front of
it live in `gglib-integration-tests`.

```text
  gglib-proxy                        gglib-engine-llama
  ┌────────────────┐  admit          ┌──────────────────────────────────────────┐
  │ resolve model  │ ──────────────▶ │ InProcessRuntime: catalog, fit, load     │
  │                │ ◀────────────── │   target = shim port, loaded n_ctx       │
  └────────────────┘  RunningTarget  └──────────────────────────────────────────┘
  ┌────────────────┐  POST /v1/chat  ┌──────────────────────────────────────────┐
  │ shape request  │ ──────────────▶ │ shim: render (gglib-gguf render_chat)    │
  │ forward        │  GET /props     │       tokenize, check n_ctx              │
  │ NormalizingSt. │  GET /slots     │       ▼                                  │
  │ usage reading  │ ◀────────────── │ Engine: one turn ──▶ decode thread       │
  └────────────────┘  llama's SSE    │                       Decoder::generate  │
                                     │       ▼                       │          │
                                     │ reply: stop strings,          ▼          │
                                     │   reasoning split       LlamaDecoder     │
                                     │                         sys (unsafe)     │
                                     └───────────────────────────────┼──────────┘
                                                                     ▼
                                                csrc/gglib_llama.c, flat C over llama.h
                                                                     ▼
                                                    llama.framework, built at the pin
```

What the shim answers, each in llama-server's shape at the pin
(`contracts/upstream/llama/`):

- `POST /v1/chat/completions`, streamed and not. A stream opens with the
  role chunk (each `prompt_progress` report rides on one when
  `return_progress` is asked), sends the reply's reasoning as
  `delta.reasoning_content` and its answer as `delta.content`, ends with the
  `finish_reason` chunk, then the `usage` chunk when
  `stream_options.include_usage` is asked, the `timings` on the last, and
  `[DONE]`. The reply is split as llama-server splits it under its default
  `--reasoning-format deepseek`: a `<think>` block at its start, or one its
  generation prompt opened, is reasoning, and the tags are not sent; a
  template with no `</think>` sends everything as the answer, and
  `reasoning_format: none` asks for that. `stop` strings are honoured, and
  the sampling keys are read as llama-server reads them. A positive
  `reasoning_budget_tokens` is read and not enforced: thinking is bounded by
  `n_predict` and the context only, where llama-server's sampler would close
  the block at the budget. A request asking for a grammar, a JSON schema, a
  structured `response_format`, tools, a `logit_bias`, its own `samplers`,
  DRY sequence breakers, log probabilities, or a `reasoning_format` other
  than `none`, `auto` or `deepseek` is refused with a 400. A prompt that
  leaves no token of the context free is refused with llama-server's
  `exceed_context_size_error`. A request whose first result is an error is
  answered with that error alone, streamed or not; a generation that fails
  after it started ends its stream with an error frame and no `[DONE]`.
- `GET /props`: the context, the template, the default sampling and the pin
  as `build_info`.
- `GET /slots`: 501, which stops gglib-proxy's slots poller for good.
  `POST /slots/{id}` is 501 too, as llama-server started without
  `--slot-save-path` answers a slot's save or restore.
- `GET /health` and `POST /apply-template`.
- Anything else: llama-server's 404, in its error envelope.

A client that hangs up mid-reply drops its stream, which raises the
generation's abort; the decoder checks it before every step.

`LlamaDecoder` memory-maps a model's weights and offloads every layer to the
GPU on an iPhone, none on its simulator, and none on a Mac unless told to. It
makes a context at the size and KV cache types the fit chose, reads a prompt
512 tokens a step, keeping the prefix the KV cache already holds, and samples
with llama-server's own chain at the pin, built from the request's sampling
keys. Its abort is checked before every decode step and, on the CPU, inside
llama.cpp's decode too; a load reports its progress and is stopped by its
abort at the next report. A load or context llama.cpp refuses fails in the
words llama.cpp logged.

[`InProcessRuntime`] admits a chat by looking its model up in the catalog,
fitting the model's context with gglib-core's `fit_verdict` against a budget
made once from the device's memory reading and held, and loading it at that
context unless it is held so already. The fit counts the weights on disk, as
the model browser does for an installed model, and no projector, which the
engine never loads. A context a request asks for lowers the rung and never
raises it. A model whose smallest rung does not fit is refused with
`ChatModelDoesNotFit` before anything loads, and one trained for fewer tokens
than that rung with words that say so. One model is held: admitting another
frees the first before the second is read, ending the reply it was
generating. Each step is an [`EngineState`] on a tokio watch (`Idle`,
`Loading` with the fraction read, `Ready`, `Generating` at the same context,
`Unloading`, `Failed`, `Stopped`), and `shutdown` stops a load under way,
frees the model, stops the shim and ends the watch.

## Internal Structure

```text
┌─────────────────────────────────────────────────────────────────────┐
│                         gglib-engine-llama                          │
├─────────────────────────────────────────────────────────────────────┤
│  decoder/   Decoder · Loaded · Step · Finished · Abort · Sampling   │
│             LlamaDecoder (llama) · ScriptedDecoder (test-decoder)   │
│  engine.rs  Engine: the model held, one generation at a time        │
│  state.rs   EngineState and the watch it is published on            │
│  render.rs  a chat request read and rendered through its template   │
│  shim/      the axum server and llama-server's JSON                 │
│  runtime/   InProcessRuntime: admission, fit, stops, shutdown       │
│  sys/       the only unsafe code: handles over the C shim (llama)   │
│  csrc/      gglib_llama.{h,c}: flat C over llama.h (llama)          │
└─────────────────────────────────────────────────────────────────────┘
```

- [`src/decoder/`](src/decoder/): the [`Decoder`] trait and what crosses it,
  [`Sampling`] with llama-server's defaults, `LlamaDecoder` and
  `ScriptedDecoder`.
- [`src/engine.rs`](src/engine.rs): [`Engine`], which loads and frees models
  and runs a generation on a thread of its own, one at a time, aborting the
  one in flight when the model changes, and publishes each step as an
  [`EngineState`].
- [`src/state.rs`](src/state.rs): [`EngineState`] and the tokio watch it is
  published on, which ends on `Stopped`.
- [`src/render.rs`](src/render.rs): a request's messages and template kwargs
  read as llama-server reads them, and rendered with
  `gglib_gguf::render_chat`; a reasoning budget of 0 renders with thinking
  off. The generation prompt, which the reply is split after, is found as
  llama-server finds it.
- [`src/shim/`](src/shim/): the server, its endpoints and the JSON they
  answer.
- [`src/runtime/`](src/runtime/): [`InProcessRuntime`], the
  `ModelRuntimePort` over the engine and its shim, and the fit of a model's
  context to the device's budget.
- [`src/sys/`](src/sys/): the crate's only unsafe code, under the `llama`
  feature: the shim's functions as `csrc/gglib_llama.h` declares them, and
  the handles that own the model, context and sampler it hands out. Each
  unsafe item carries its own `#[expect(unsafe_code, reason = "...")]`.
- [`csrc/`](csrc/): the C shim, compiled by build.rs against the pinned
  `llama.h`. A function the pin renames, retypes or deprecates is a compile
  error there, and `scripts/apple/check-llama-xcframework.sh` holds the
  framework to every `llama_*` function it calls.

`make engine-llama-test` runs the tests with the decoder over llama.h on the
Mac's slice (the real-model ones read `GGLIB_ENGINE_MODELS_DIR`), and
`make engine-llama-clippy` lints it for the Mac, the iPhone and its
simulator; CI's `phone-llama` job runs both. `make record-upstream-inproc`
records the shim over that decoder, asked what gglib-runtime's contract
recorder asks the pinned llama-server, into `contracts/upstream/llama-inproc/`,
which gglib-runtime's tests hold to llama-server's recording on every OS.

## Usage

```rust,ignore
use gglib_engine_llama::InProcessRuntime;

// One reading of the device's memory, held for the serve.
let runtime = Arc::new(InProcessRuntime::start(decoder, catalog, reading).await?);
let mut states = runtime.subscribe();
// gglib_proxy::serve(ServeConfig { runtime_port: runtime.clone(), .. }) admits
// each chat through it and forwards to the shim it names.
runtime.shutdown().await?;
```
