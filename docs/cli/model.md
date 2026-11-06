# gglib model

The library. A model is a GGUF file gglib has read: its architecture,
quantization, trained context, whether it sees images, and tags for whether
it reasons and how its chat template writes tool calls. Every other command
names a model by the id or name shown here.

<!-- snippet: model-list -->
```console
$ gglib model list
Found 3 model(s):

ID  Name                      Model ID               Params   ⚡ t/s      Arch         Quant    Context    Images  Draws Added                File Path
---------------------------------------------------------------------------------------------------------------------------------------------------------------------
3   Qwen3.8-27B               Qwen3.8-27B            27.0     --         qwen35       Q8_0     262144     --      --    2026-10-31 06:25:37  /Users/mmogr/Library/Application Support/gglib/models/q38/Qwen3.8-27B-Q8_0.gguf
2   Qwen3-4B-Instruct-2507    Qwen3-4B-Instruct-2507 4.0      --         qwen3        Q8_0     262144     --      --    2026-10-31 06:25:37  /Users/mmogr/Library/Application Support/gglib/models/Qwen3-4B-Instruct-2507-Q8_0.gguf
1   Qwen3-30B-A3B             Qwen3-30B-A3B          30.0     --         qwen3moe     Q4_K_M   40960      --      --    2026-10-31 06:24:05  /Users/mmogr/Library/Application Support/gglib/models/Qwen3-30B-A3B-Q4_K_M.gguf
```
<!-- /snippet -->

While this machine is paired, `list` ends with one line on the paired
machine, connected, away or not, without asking that machine anything;
`list --remote` lists that machine's models by their ids there, and
`inspect --remote` reads one of them.

## `add`, `list`, `remove`, `update`

The library itself. `add <file>` reads a GGUF where it lies and registers
it; nothing is copied. `list` sorts by added, name, params, or benchmark
speed.

- `remove` drops a model's row and leaves its file. It refuses a model that
  is being served under the same data root, and says how to stop it;
  `-y` / `--yes` skips the confirmation and stops nothing.
- `update` changes a model's metadata, including the companion files of an
  image model (below), and the model's own launch defaults: `--jinja
  auto|on|off` for chat templates and `--mtp auto|on|off` for speculative
  decoding, each below what a launch itself asks for and above the model's
  tags. [`explain`](#explain) shows the flag each one puts on the command
  line, and that it was yours.

## `inspect`

Everything gglib read from one file, and what it derived: tags, capability
flags, and whether the model is being served, on which port.

<!-- snippet: model-inspect -->
```console
$ gglib model inspect Qwen3-4B-Instruct-2507
------------------------------------------------------------
  Model: Qwen3-4B-Instruct-2507
------------------------------------------------------------
  ID             : 2
  File           : /Users/mmogr/Library/Application Support/gglib/models/Qwen3-4B-Instruct-2507-Q8_0.gguf
  Projector      : none (text only)
  Parameters     : 4.0B
  Architecture   : qwen3
  Quantization   : Q8_0
  Context Length : 262144 tokens

  Tags
------------------------------------------------------------
  reasoning, agent, format:qwen-xml

  Capabilities
------------------------------------------------------------
  supports-system-role  : yes
  requires-strict-turns : no
  supports-tool-calls   : yes
  supports-reasoning    : yes

  Timestamps
------------------------------------------------------------
  Added          : 2026-10-31 06:25:37 (7 hours ago)
------------------------------------------------------------
```
<!-- /snippet -->

- `--metadata` adds the raw GGUF key-value metadata; `--json` prints the
  whole record for a script.
- `inspect` names an image model's family and each component, linked or
  missing.

## `explain`

The launch gglib would make for the model, without making it: the command
it would run, each argument beside whose choice it was (the model author's,
gglib's, or yours) and why, then the context size the model would be served
at and how it was fitted. When a model is loaded and this one would not fit
beside it, one more line says which model the launch would unload first.
Then every resolved sampling parameter, beside the layer that supplied it.
All of it comes from the same code a launch runs, so it cannot describe a
launch or a hierarchy different from the one that happens, and
`GET /api/models/{id}/explain` answers with the same thing.

<!-- snippet: model-explain -->
```console
$ gglib model explain Qwen3-4B-Instruct-2507

  Launch for Qwen3-4B-Instruct-2507 (id 2)
  llama-server -m /Users/mmogr/Library/Application Support/gglib/models/Qwen3-4B-Instruct-2507-Q8_0.gguf --host 127.0.0.1 --port auto --metrics --parallel 1 -c 131072 --jinja --reasoning-format deepseek --cache-ram 100813 --cache-type-k q8_0 --cache-type-v q8_0

  -m                  /Users/mmogr/Library/Application Support/gglib/models/Qwen3-4B-Instruct-2507-Q8_0.gguf ← model author: the weights, as published
  --host              127.0.0.1    ← gglib: loopback only; the endpoint is the address you share
  --port              auto         ← gglib: the next free port above the base when it loads; the endpoint's port does not move
  --metrics                        ← gglib: the dashboard reads them
  --parallel          1            ← gglib: one request at a time per model, so each gets the whole context; the queue holds the rest
  -c                  131072       ← gglib: the largest rung that fits beside the weights on this machine
  --jinja                          ← gglib: tagged agent: tool calls need the model's own template
  --reasoning-format  deepseek     ← gglib: tagged reasoning: thinking is split from the answer
  --cache-ram         100813       ← gglib: sized when loaded: free RAM after the weights, the KV cache and the other loaded models
  --cache-type-k      q8_0         ← gglib: q8_0 halves the KV cache at no measured cost
  --cache-type-v      q8_0         ← gglib: q8_0 halves the KV cache at no measured cost

Context
  serves at              131072 (fitted to hardware)
  fitted to hardware     131072
    device budget        96.00 GiB
    weights              3.99 GiB
    kv bytes/token       78336
    trained window       262144
    before snapping      262144

  Sampling for Qwen3-4B-Instruct-2507 (id 2)
-------------------------------------------------------------------------------------
  temperature             1.0     ← per-model defaults (auto-detected: reasoning tag)
  top_p                   0.95    ← per-model defaults (auto-detected: reasoning tag)
  top_k                   20      ← per-model defaults (auto-detected: reasoning tag)
  presence_penalty        1.5     ← per-model defaults (auto-detected: reasoning tag)
  repeat_penalty          1.0     ← per-model defaults (auto-detected: reasoning tag)
  min_p                   0.0     ← per-model defaults (auto-detected: reasoning tag)
  frequency_penalty       —       ← unset by design
  dynatemp_range          —       ← unset by design
  dynatemp_exponent       —       ← unset by design
  top_n_sigma             —       ← unset by design
  dry_multiplier          —       ← unset by design
  dry_base                —       ← unset by design
  dry_allowed_length      —       ← unset by design
  dry_penalty_last_n      —       ← unset by design
  max_tokens              32768   ← per-model defaults (auto-detected: reasoning tag)
  reasoning_effort        —       ← unset by design
  reasoning_budget_tokens —       ← unset by design
-------------------------------------------------------------------------------------
  Operator flags (gglib proxy --temperature, ...) outrank every layer above.
  Client sampling is ignored, except max_tokens, reasoning_budget_tokens.

```
<!-- /snippet -->

The layers, from strongest to weakest, are in
[Sampling resolution](../sampling.md).

## `load`, `unload`

The app's Load and Unload, from the terminal. `load` asks the daemon to
start the model's server, llama-server or sd-server for an image model,
and the daemon keeps it resident after the command returns; `list` then
shows it serving. A model already loaded is reused. `--context N` or
`--context max` sets the context it is served at; omitted, the daemon
fits one. `--pin` pins the endpoint to the model first, as `gglib serve`
does, so every request for another model is refused while the pin holds;
an endpoint already running unpinned is the daemon's refusal, and
`gglib proxy stop` clears it. `unload` asks the daemon to stop the model's
server; one that is not loaded is said so, in the daemon's words.

Illustrative, not a recording: the two lines are the daemon's sentences.

```console
$ gglib model load 3
  Server started on port 9301
$ gglib model unload 3
  Server for model 3 stopped
```

## `download`, `search`, `browse`

Hugging Face. `search <query>` and `browse` (popular repositories) find a
model; `download <repo>` fetches it, `-q <QUANT>` picks a quantization, and
`--list-quants` lists the ones a repository offers, each with whether it
fits this machine, judged as a download without `-q` picks one (the same
listing the GUI's table shows).

<!-- snippet: model-search -->
```console
$ gglib model search "gemma 3 instruct" --limit 5
🔍 Searching HuggingFace Hub for 'gemma 3 instruct'...

📋 Found 5 models:
────────────────────────────────────────────────────────────────────────────────
 1. DavidAU/gemma-4-31B-it-Mystery-Fine-Tune-HERETIC-UNCENSORED-Thinking-Instruct-GGUF (↓2.6K ❤65)
    Quantizations: Q4_K_S, Q6_K

 2. lm-kit/gemma-3-4b-instruct-gguf (↓1.9K ❤4)
    Quantizations: F16, Q2_K, Q3_K_L, Q3_K_M, Q3_K_S, Q4_K_M, Q4_K_S, Q5_K_M, Q5_K_S, Q6_K, Q8_0

 3. mradermacher/gemma-3-12b-it-vl-GLM-4.7-Flash-INSTRUCT-Thinking-Hybrid-Heretic-Uncensored-i1-GGUF (↓1.8K ❤3)
    Quantizations: IQ1_M, IQ1_S, IQ2_M, IQ2_S, IQ2_XS, IQ2_XXS, IQ3_M, IQ3_S, IQ3_XS, IQ3_XXS, IQ4_NL, IQ4_XS, Q2_K, Q3_K_L, Q3_K_M, Q3_K_S, Q4_0, Q4_1, Q4_K_M, Q4_K_S, Q5_K_M, Q5_K_S, Q6_K

 4. mradermacher/gemma-3-12b-it-vl-Polaris-GLM-4.7-Flash-VAR-Thinking-Instruct-Heretic-Uncensored-i1-GGUF (↓1.5K ❤6)
    Quantizations: IQ1_M, IQ1_S, IQ2_M, IQ2_S, IQ2_XS, IQ2_XXS, IQ3_M, IQ3_S, IQ3_XS, IQ3_XXS, IQ4_NL, IQ4_XS, Q2_K, Q3_K_L, Q3_K_M, Q3_K_S, Q4_0, Q4_1, Q4_K_M, Q4_K_S, Q5_K_M, Q5_K_S, Q6_K

 5. mradermacher/Gemma3-27B-it-vl-Polaris-HI16-Heretic-Uncensored-INSTRUCT-i1-GGUF (↓1.3K ❤1)
    Quantizations: IQ1_M, IQ1_S, IQ2_M, IQ2_S, IQ2_XS, IQ2_XXS, IQ3_M, IQ3_S, IQ3_XS, IQ3_XXS, IQ4_XS, Q2_K, Q3_K_L, Q3_K_M, Q3_K_S, Q4_0, Q4_1, Q4_K_M, Q4_K_S, Q5_K_M, Q5_K_S, Q6_K

💡 To download a model: gglib model download <model_id>
💡 To list quantizations: gglib model download <model_id> --list-quants
```
<!-- /snippet -->

Downloads run on the daemon, through the queue the desktop app uses, so a
download started here shows there and the reverse. In a terminal the command
becomes a live monitor:

<p align="center">
  <img src="../assets/cli-model-download.gif" alt="gglib model download: the queue monitor, from the first bytes to the registered file" width="760">
</p>

- **[a]** queues another model while one is in flight; **[q]**, `Esc` or
  `Ctrl-C` drains the queue once, and a second press force-quits. The bar
  shows the phase: `Downloading` → `Finalizing` → `Registering` →
  `Completed`, `Failed` or `Cancelled`. Without a terminal the monitor is
  plain lines.
- Transfers are native HTTP, resumable and checksum-verified; no Python is
  needed. `gglib config fast-downloads` adds the optional `hf_xet`
  accelerator ([config](config.md#fast-downloads)).
- A download that comes with a projector links it, so the model reads images
  from day one. An image model's `download` brings its family's companions
  too: before it queues, the command names them with their sizes, those
  already here and the bytes to fetch, and its last line says which were
  linked.
- `search --images` and `browse --images` look for image models.
- Private and gated repositories need `HF_TOKEN` in the environment. A
  command and the daemon each read it once, as they start: `search`,
  `browse`, `check-updates` and `upgrade` ask the Hub with the command's own,
  while a queued download, a repair's among them, runs on the daemon and uses
  the daemon's, so the daemon must have been started with it.
  `download --list-quants` takes `--token` instead.

The transfer engine is described in the
[download module](../../crates/gglib-cli/src/handlers/model/download/README.md).

## `check-updates`, `upgrade`

`check-updates --all`, or `--identifier <model>`, asks each downloaded
model's repository for its latest commit and reports an update when that
differs from the commit recorded at download (any commit counts, a README
edit among them); `upgrade <model>` replaces the file with that revision.

- Like `remove`, `upgrade` refuses a model that is being served under the
  same data root, before it asks the Hub anything; `-y` skips the
  confirmation and stops nothing.

## `verify`, `repair`

`verify` computes SHA-256 over a model's files and compares them with what
was recorded. `repair` re-downloads the shards that fail.

- `repair` runs on the daemon, as a download does: the daemon deletes the
  model's unhealthy files and queues their download, and the command watches
  it to its end. It fails when the download does, naming the files still
  missing and the `gglib model download` command that fetches them.

## `retag`, `capabilities`

What gglib derived from the file, and how to change it.

- `retag` re-derives the auto-tags from persisted GGUF metadata. Run it
  when a release adds a tag, so the library gains it: the `format:*`
  dialect family, for one. It is additive by default and never touches
  user-curated tags; `--full` rebuilds the auto namespace only.
- `capabilities <model>` shows the capability flags (system role, strict
  turns, tool calls, reasoning); `--set` and `--unset`, repeatable,
  override them.

[Tags & capability detection](../tags.md) explains each tag and flag.

## Image models

An image model (Flux.1, SDXL, Qwen-Image 2.1, Z-Image, FLUX.3 Image Klein
or FLUX.2 klein, known from its tensor names) draws with companion files
beside its weights.

- A file's name picks its variant: with "turbo" in it, in any case, a
  Qwen-Image 2.1 file draws as Turbo (its 8-step sigma schedule, guidance
  1.0) and a Z-Image file as Z-Image Turbo (8 steps, guidance 1.0); without
  it, Qwen-Image 2.1 draws as base and Z-Image as base Z-Image (28 steps,
  guidance 5.0).
- FLUX.3 Image Klein draws at 16 steps, guidance 1.0, with the FLUX.2 VAE
  and Qwen3-VL 4B. A Klein file with "base" in its name, in any case, is
  Klein base, which is not drawn: a request for it is refused.
- FLUX.2 klein (the 4B model) draws at 4 steps, guidance 1.0, with the
  FLUX.2 VAE and Qwen3 4B. A FLUX.2 klein file with "base" in its name, in
  any case, is klein base, which is not drawn: a request for it is refused.
- `list` names a model's family in the `Draws` column, with the components
  it still needs; `inspect` names the family and each component, linked or
  missing.
- `add` and `update` link a component with `--component <role>=<path>`
  (`vae`, `clip_l`, `t5xxl` or `llm`, repeatable) and unlink one with
  `--no-component <role>`. A file is linked only when its tensors are that
  role's for the model's family.

Drawing is [`gglib image`](image.md).

## Flags

`gglib model <subcommand> --help` lists every flag.
