# Changelog

## [0.36.0-rc.1](https://github.com/mmogr/gglib/compare/v0.35.0-rc.3...v0.36.0-rc.1) (2026-11-13)


### What gglib now does

* **agent:** a device's run waits for an agent slot instead of a 429 ([#1922](https://github.com/mmogr/gglib/issues/1922)) ([7c909ea](https://github.com/mmogr/gglib/commit/7c909ea0cfb0fa59de249355b93e0d825f7c9cc6)), closes [#1528](https://github.com/mmogr/gglib/issues/1528)
* **cli:** config paths and install.sh report every gglib on PATH and the desktop app ([#1923](https://github.com/mmogr/gglib/issues/1923)) ([9f3e501](https://github.com/mmogr/gglib/commit/9f3e5010135197856ee9b2c87bafbad423e18dd9))
* **runtime:** a launch's narration carries the fit its context was sized by ([#1924](https://github.com/mmogr/gglib/issues/1924)) ([6d1ad6b](https://github.com/mmogr/gglib/commit/6d1ad6bba40804eea5d67afdc678a635329273d0))


### Documentation

* **compat:** the promise names its CLI and settings list and its oldest library ([#1920](https://github.com/mmogr/gglib/issues/1920)) ([2609c74](https://github.com/mmogr/gglib/commit/2609c748aae95d915ec9a22a38b22d535830f02f))


### Internal

* **deps:** bump taiki-e/install-action ([#1917](https://github.com/mmogr/gglib/issues/1917)) ([0ea9ba1](https://github.com/mmogr/gglib/commit/0ea9ba1a3d829e735c3a1da45f95179c686148ee))

## [0.35.0-rc.3](https://github.com/mmogr/gglib/compare/v0.35.0-rc.2...v0.35.0-rc.3) (2026-11-13)


### ⚠ BREAKING CHANGES

* **proxy:** a streamed `/v1/chat/completions` reply is llama-server's own chunks.
    - Every chunk comes through one for one with all its fields: its `id`, a `created` that can change between chunks, `system_fingerprint`, the opening chunk with `delta.role: "assistant"` and `content: null`, and `timings`, `logprobs` and any other key it writes.
    - A chunk the proxy edits is written with its keys in alphabetical order. `model` still names the served model.
    - A client that sends `return_progress: true` gets llama-server's progress chunks as written, which carry `choices`. llama-server's `:` keep-alive comments pass through.
    - A mid-stream llama-server error keeps its whole object (`n_ctx` and the like), with `code` set to `"upstream_error"` unless upstream's code was a string.
    - A stream cut off without a finish no longer ends with a gglib-made `finish_reason: null` chunk. Tool calls held for repair on such a stream go out unrepaired.
    - gglib's own frames carry the reply's upstream `id` and its latest `created`.
    - Forwarded frames are larger, so a run's 8 MB log holds about 18% fewer frames.
    - gglib-core API: `SseEncoder::with_reading`, `parse_sse_frame` and `SseParseResult` are gone (use `sse::events_of` on the parsed JSON). `SseEncoder::encode` returns `None` for reasoning, prompt-progress, usage and upstream-error events.
* **proxy:** gglib no longer translates tool-call dialects; llama-server's own parsing is what clients get.
    - Tool-call markup llama-server doesn't parse (Qwen/Hermes `<tool_call>`, template-derived dialects) now reaches clients as `content`, streamed or not, with no `call_qwen_N`/`call_dialect_N` ids. A malformed call is no longer replaced by the "unparsed tool-call output" notice. Stray `<think>`/`</think>` tags in `content` are no longer removed. A tool-call stream that ends `finish_reason: "stop"` is no longer rewritten to `"tool_calls"`.
    - `tool_choice: "required"` or a named function is passed to llama-server as sent. It is no longer rewritten to `"none"` under a gglib grammar. Repair no longer judges a turn the client sent as `"required"`.
    - `GGLIB_DISABLE_GRAMMAR`, `GGLIB_DISABLE_DIALECT_PARSER` and `GGLIB_MAX_GRAMMAR_TOOL_CALLS` are ignored; unset them.
    - Dashboard: `recent_requests[].grammar_enforced`, the `constrained` intervention and `normalization_errors` are gone. `dialect_residue` scans the built-in markers only.
    - Agent runs: a reply cut off inside dialect markup is no longer `call_cut`, and the "LLM normalization issue" warning is gone.
    - Benchmark: the gglib arm no longer includes the grammar or parser, so its results can't be compared with earlier reports.
    - Library: opening a library with this build strips every `format:*` tag and drops the `dialect_spec` column, and the library becomes schema generation 8, which older gglib builds refuse. The backup taken before the upgrade (`<name>.gen<N>.bak`) is the way back. Detection no longer adds `format:*` tags.
    - Library API: gglib-core loses `DialectSpec`, `BodyCodec`, `EmissionProfile` and the rest of `domain::dialect`, the `normalize::tags` constants, `is_system_tag`, `Model`/`NewModel.dialect_spec`, `ModelSummary.dialect`, `GgufCapabilities.dialect` and `.extensions`, and `RetagDiff.spec_changed`. The retag response loses `specChanged`.
    - Output: the launch banner and the GUI's "Tool calls" row no longer name a dialect. `gglib model retag` no longer prints "dialect spec re-derived", and `gglib model add --reimport` says "Replaced: tags, capabilities."

### What gglib now does

* **proxy:** the proxy forwards llama-server's own stream frames ([#1918](https://github.com/mmogr/gglib/issues/1918)) ([2873479](https://github.com/mmogr/gglib/commit/2873479cf635b9742e9beb012f099da4b2c01431))


### What it stopped getting wrong

* **hf:** a repository's parameter count is not taken from its draft ([cc3a466](https://github.com/mmogr/gglib/commit/cc3a4660f9709b24b6f05e899ffb6753713e871c)), closes [#1740](https://github.com/mmogr/gglib/issues/1740)
* **hf:** search and the inspector say what a repository holds ([#1912](https://github.com/mmogr/gglib/issues/1912)) ([cc3a466](https://github.com/mmogr/gglib/commit/cc3a4660f9709b24b6f05e899ffb6753713e871c))
* **hf:** search lists only quantisations, each spelt one way ([cc3a466](https://github.com/mmogr/gglib/commit/cc3a4660f9709b24b6f05e899ffb6753713e871c)), closes [#1860](https://github.com/mmogr/gglib/issues/1860)


### Internal

* **benchmark:** agentic verdicts are served, not re-derived in the GUI ([12ff957](https://github.com/mmogr/gglib/commit/12ff95708b582bf0b11054bc56d47fd7ac31b9ae))
* **cli:** the promised commands, flags and settings keys are a checked-in list ([#1913](https://github.com/mmogr/gglib/issues/1913)) ([2784d5d](https://github.com/mmogr/gglib/commit/2784d5d022c7e3e98a4285be263454b47ca575e5))
* **core:** the unload sentence and sampling source labels are served ([12ff957](https://github.com/mmogr/gglib/commit/12ff95708b582bf0b11054bc56d47fd7ac31b9ae))
* **gui:** engine, verdict and unload sentences are served, not re-derived ([#1914](https://github.com/mmogr/gglib/issues/1914)) ([12ff957](https://github.com/mmogr/gglib/commit/12ff95708b582bf0b11054bc56d47fd7ac31b9ae))
* **hf:** the daemon always names the picked variant ([cc3a466](https://github.com/mmogr/gglib/commit/cc3a4660f9709b24b6f05e899ffb6753713e871c))
* **proxy:** gglib stops translating tool-call dialects ([#1916](https://github.com/mmogr/gglib/issues/1916)) ([acdba28](https://github.com/mmogr/gglib/commit/acdba283c35f2512b331cf76b4ca7b9d7d56fe70))
* **scripts:** remove the file-size check ([#1911](https://github.com/mmogr/gglib/issues/1911)) ([0b42c97](https://github.com/mmogr/gglib/commit/0b42c971a053e5b0918c5b44416a0b5eb8898916))
* **settings:** an engine's pin sentence is the status command's, served ([12ff957](https://github.com/mmogr/gglib/commit/12ff95708b582bf0b11054bc56d47fd7ac31b9ae))
* slot, duration, sharing and reach words are served ([#1915](https://github.com/mmogr/gglib/issues/1915)) ([812ac04](https://github.com/mmogr/gglib/commit/812ac04c6dde9931d8e93b0ac4b299c8c9a5e7cc)), closes [#1832](https://github.com/mmogr/gglib/issues/1832) [#1833](https://github.com/mmogr/gglib/issues/1833)

## [0.35.0-rc.2](https://github.com/mmogr/gglib/compare/v0.35.0-rc.1...v0.35.0-rc.2) (2026-11-12)


### ⚠ BREAKING CHANGES

* **proxy:** the proxy's cache-clear and shutdown refusals, a bad `X-Gglib-Session-Id`, unknown paths and methods, and unreadable path segments, queries and bodies now answer OpenAI's error object `{"error": {"message", "type", "code"}}` with a code, instead of a bare `{"error": "text"}`, plain text or an empty body. axum's 415 and 422 rejections are now 400 `invalid_request`.
* **proxy:** every gglib-own route moved from `/v1/<x>` to `/v1/gglib/<x>` (chats, attachments, models/{name}/load and /detail, images/drawing, proxy/status, proxy/cache/clear, proxy/shutdown, runs). `/v1/models`, `/v1/chat/completions`, `/v1/embeddings` and `/v1/images/generations` don't move. Update the laptop and the phone (ggchat) before the Mac: a device on the old paths can't reach a moved Mac.

### What gglib now does

* **agent:** a paired machine's refusal reaches a run as its code ([#1905](https://github.com/mmogr/gglib/issues/1905)) ([07802ce](https://github.com/mmogr/gglib/commit/07802ce4f5c28291766dfe64403917164da801d8)), closes [#1256](https://github.com/mmogr/gglib/issues/1256)
* **proxy:** /v1/models/{name} answers the model ([#1908](https://github.com/mmogr/gglib/issues/1908)) ([a659962](https://github.com/mmogr/gglib/commit/a65996279848186babe7ad2c5a44f0bff9b779e0))
* **proxy:** devices say their gglib version and what they serve ([#1902](https://github.com/mmogr/gglib/issues/1902)) ([46bc17a](https://github.com/mmogr/gglib/commit/46bc17a9bb6133c0ce5613553e145ba6764199fa))
* **proxy:** gglib's own routes live under /v1/gglib ([#1907](https://github.com/mmogr/gglib/issues/1907)) ([7be248a](https://github.com/mmogr/gglib/commit/7be248a263e5eba612a14bc16589d422cf2995f3))


### What it stopped getting wrong

* **cli:** model list and proxy trips size their columns to what they print ([#1903](https://github.com/mmogr/gglib/issues/1903)) ([70d5e26](https://github.com/mmogr/gglib/commit/70d5e26752426bbfa708d01b50945f7094f96bed))
* **proxy:** every proxy answer is OpenAI's error shape with a code ([#1909](https://github.com/mmogr/gglib/issues/1909)) ([097f60d](https://github.com/mmogr/gglib/commit/097f60d0ae646aa7e395fcde4d1352b2ec94d9a8))


### Internal

* **release:** a breaking change after 1.0.0 starts the next major ([#1904](https://github.com/mmogr/gglib/issues/1904)) ([87c48fe](https://github.com/mmogr/gglib/commit/87c48fed8d32b361fccb8587acfa00254122f6f4))

## [0.35.0-rc.1](https://github.com/mmogr/gglib/compare/v0.34.0-rc.1...v0.35.0-rc.1) (2026-11-12)


### ⚠ BREAKING CHANGES

* **proxy:** `gglib config set loop-guard-mode off|note|refuse` is now `gglib config set loop-guard true|false` (loopGuard on the wire). refuse is gone: a library that stored it is upgraded to the guard on. The trip log's mode column and LoopGuardTripDay.mode are gone.
* **cli:** flags are renamed or removed, and each short letter means one thing (`-c -f -m -n -o -p -q -s -v -y`). `--ctx-size` and `--context` are `--context-size`; `proxy --default-context` is `--default-context-size`; chat/q `--max-iterations` and `--max-parallel` are `--max-tool-iterations` and `--max-parallel-tools`. Removed: chat/q's twelve ignored sampling flags, `model download --skip-db` and `--token` (set `HF_TOKEN` for `--list-quants`), `config check-deps --setup-fast-downloads` (use `config fast-downloads enable`), `remote enable --allow-mcp` (use `mcp allow-remote`), `benchmark tune --apply-best` (use `--apply`), `model search --sort modified`, and install's `update` alias. Other short flags are long-only now (for example `q -Q` is `--quiet`, `model search -l` is `-n`). `mcp add` takes no `--type` or `--url`; stored SSE MCP servers are deleted on upgrade, and an MCP server's lifecycle is `eager` or `lazy`. `/api`: the download queue takes `quantization` only, start-server takes `id` only.
* **sampling:** the `agentic-sampling` settings key and the `GGLIB_DISABLE_AGENTIC_SAMPLING` switch are gone; drop them from scripts and environments. Tool turns are no longer capped at 0.3. To keep a lower temperature on them, set it in a profile, on the model or globally.
* **gui:** the desktop app is now gglib (gglib.app, program file gglib-gui). A manual macOS install leaves the old GGLib GUI.app behind: delete it. On Windows the app installs as gglib-gui (its install folder must not be gglib's data folder), beside the old GGLib GUI rather than over it: uninstall GGLib GUI. With Start at login on, the new app registers a login item of its own and the old one stays behind: remove the GGLib GUI entry once (macOS: ~/Library/LaunchAgents/GGLib GUI.plist; Linux: ~/.config/autostart/GGLib GUI.desktop; Windows: Startup apps), or the old app starts too. apt users run `sudo apt install gglib-gui` once, which replaces gg-lib-gui; dnf swaps it on upgrade. Data folders are keyed by com.gglib.gui and are untouched.
* **cli:** `gglib config settings <cmd>` is now `gglib config <cmd>`, and each `--flag value` of `settings set` is `set flag value` (`--default-image-model` is `set default-image-model-id`). `config default <model>` is `config set default-model-id <model>`, `--clear` is `config unset default-model-id`. `config models-dir set <path>` is `config set models-dir <path>`, `show` is `config show`, `prompt` and `--no-create` are gone. `config profile rm` is `config profile remove`.
* **daemon:** `gglib daemon run --share-lan` and `gglib web --share-lan` are removed, and the daemon no longer accepts the proxy's API key. Open the web page on the machine the daemon runs on, from the link `gglib web` prints. Other machines reach the models through the proxy (`gglib proxy --host 0.0.0.0`, which asks its key) or `gglib remote`. A client that sent the proxy key to `/api` must run as the owner on this machine, where it can read the daemon token.

### What gglib now does

* **cli:** every flag has one name, every short letter one meaning ([eeb66a9](https://github.com/mmogr/gglib/commit/eeb66a9a3584b823c847fbabb8d5e89c496a43ca))
* **cli:** settings are set by key with gglib config set ([#1893](https://github.com/mmogr/gglib/issues/1893)) ([ee24078](https://github.com/mmogr/gglib/commit/ee240782c4c46c7e5e433d0abedbc3a8edb3bd8d))
* **daemon:** the daemon listens on this machine only ([#1892](https://github.com/mmogr/gglib/issues/1892)) ([b083440](https://github.com/mmogr/gglib/commit/b083440dd4c0c385650ca45b2d252f9e8aef34b7))
* **gui:** the desktop app is called gglib ([#1894](https://github.com/mmogr/gglib/issues/1894)) ([19f44df](https://github.com/mmogr/gglib/commit/19f44df2df1f5f7149f975b06df5d3e3a939f52c))
* **proxy:** the loop guard is on or off ([#1901](https://github.com/mmogr/gglib/issues/1901)) ([66524d8](https://github.com/mmogr/gglib/commit/66524d8e649b616b7da0df9aeffd5993d8767f2a))
* **sampling:** tool turns keep the temperature the ladder resolved ([#1896](https://github.com/mmogr/gglib/issues/1896)) ([0f15fa8](https://github.com/mmogr/gglib/commit/0f15fa8c591f2c6755ae17d18a62a84c02dfda09))


### What it stopped getting wrong

* **core:** a chat keeps settings it can't read when one is changed ([#1900](https://github.com/mmogr/gglib/issues/1900)) ([f291be1](https://github.com/mmogr/gglib/commit/f291be1a67788d3944769078ae0c7df8329fe584))


### Internal

* **sampling:** a template's capabilities are read, never used to edit the request ([#1897](https://github.com/mmogr/gglib/issues/1897)) ([54842d7](https://github.com/mmogr/gglib/commit/54842d7e52476d27110f1bb156f7bb22d7aa6251))

## [0.34.0-rc.1](https://github.com/mmogr/gglib/compare/v0.33.1-rc.1...v0.34.0-rc.1) (2026-11-12)


### What gglib now does

* **db:** back up the library before an upgrade changes it ([#1887](https://github.com/mmogr/gglib/issues/1887)) ([fff6174](https://github.com/mmogr/gglib/commit/fff6174bcbb5e5ebf4d89c60164a91707e000f99))


### What it stopped getting wrong

* **db:** an edit keeps a stored model setting this build cannot read ([#1885](https://github.com/mmogr/gglib/issues/1885)) ([06c209e](https://github.com/mmogr/gglib/commit/06c209ef56313b0bf49ef6d305582e5321ce33f0))
* **paths:** the daemon stores the models folder the first time it starts ([#1886](https://github.com/mmogr/gglib/issues/1886)) ([efcdd3c](https://github.com/mmogr/gglib/commit/efcdd3c950b7616a2a7c223f1f9237ed82e5c8f5))


### Internal

* **db:** a library made by gglib 0.33.0 opens and reads back whole ([#1890](https://github.com/mmogr/gglib/issues/1890)) ([22c8f4c](https://github.com/mmogr/gglib/commit/22c8f4ca69b2e8b488cf832b28725b3f86aa36d7))
* **db:** one numbered list of migrations ([#1888](https://github.com/mmogr/gglib/issues/1888)) ([b2cd138](https://github.com/mmogr/gglib/commit/b2cd13889ea672a917cae8b91f73a0b88cb5649a))
* **frontend:** the library search tests wait for the pick to render ([#1891](https://github.com/mmogr/gglib/issues/1891)) ([4799a0d](https://github.com/mmogr/gglib/commit/4799a0dd1a7d3f9c64f7867837afe136254d2914))

## [0.33.1-rc.1](https://github.com/mmogr/gglib/compare/v0.33.0-rc.1...v0.33.1-rc.1) (2026-11-12)


### What it stopped getting wrong

* **fit:** size the KV cache per layer, so Gemma 4 fits 128K, not 16K ([#1882](https://github.com/mmogr/gglib/issues/1882)) ([e329c54](https://github.com/mmogr/gglib/commit/e329c543c39ff8f4ddbb49ab88bc3551035d0ac7))
* **gguf:** a stored header keeps per-layer lists, and a reimport refreshes it ([#1881](https://github.com/mmogr/gglib/issues/1881)) ([0d025a6](https://github.com/mmogr/gglib/commit/0d025a6fc3bf67abe19f0af9b0a4bf43fcd2a53e))
* **proxy:** a reply names the served model, and keep-alives don't hold a stalled reply open ([#1880](https://github.com/mmogr/gglib/issues/1880)) ([f5373e5](https://github.com/mmogr/gglib/commit/f5373e5c8167704e0a528431ce73f1893f9c4ecb))
* **proxy:** every reader of a stored key trims it, so a padded key admits ([#1879](https://github.com/mmogr/gglib/issues/1879)) ([9a0ac7e](https://github.com/mmogr/gglib/commit/9a0ac7e8e3a3e8ce8e6100f50e1a16eb02a3a3a0))


### Documentation

* say what a version promises ([#1878](https://github.com/mmogr/gglib/issues/1878)) ([0039a8d](https://github.com/mmogr/gglib/commit/0039a8d0476e64eed2b55ef62568e30dd8f0632f))

## [0.33.0-rc.1](https://github.com/mmogr/gglib/compare/v0.32.1-rc.1...v0.33.0-rc.1) (2026-11-10)


### ⚠ BREAKING CHANGES

* **models:** an unknown parameter count is stored and shown as unknown, not as 0.0B ([#1875](https://github.com/mmogr/gglib/issues/1875))
* **gguf:** a model's parameter count is its tensors', so a 177B MoE is no longer listed as 28672B ([#1873](https://github.com/mmogr/gglib/issues/1873))
* **models:** a one-file repository's download shows the header's type as its quantization ([#1874](https://github.com/mmogr/gglib/issues/1874))

### What it stopped getting wrong

* **gguf:** a model's parameter count is its tensors', so a 177B MoE is no longer listed as 28672B ([#1873](https://github.com/mmogr/gglib/issues/1873)) ([70aff41](https://github.com/mmogr/gglib/commit/70aff4123fc5000bf05fd1e3b2570819ce8228a5)), closes [#1870](https://github.com/mmogr/gglib/issues/1870)
* **models:** a one-file repository's download shows the header's type as its quantization ([#1874](https://github.com/mmogr/gglib/issues/1874)) ([4ffb065](https://github.com/mmogr/gglib/commit/4ffb0659e730a32cde883db2fe18961111fdab8e))
* **models:** an unknown parameter count is stored and shown as unknown, not as 0.0B ([#1875](https://github.com/mmogr/gglib/issues/1875)) ([563b19f](https://github.com/mmogr/gglib/commit/563b19f9bfa16edbbc0e64bf2854b8858c12693f)), closes [#1872](https://github.com/mmogr/gglib/issues/1872)

## [0.32.1-rc.1](https://github.com/mmogr/gglib/compare/v0.32.0-rc.1...v0.32.1-rc.1) (2026-11-10)


### Internal

* **deps:** bump the cargo and npm dependencies to the newest versions their ranges allow ([#1869](https://github.com/mmogr/gglib/issues/1869)) ([7ec1f1b](https://github.com/mmogr/gglib/commit/7ec1f1b35a3e9fc4df95ce91a1cb94eadf09bf48))
* **deps:** bump the cargo-compatible group across 1 directory with 13 updates ([#1867](https://github.com/mmogr/gglib/issues/1867)) ([2d2b11d](https://github.com/mmogr/gglib/commit/2d2b11db7a983edf9fdd0d269bc7dbbc602a05d7))
* **deps:** move llama.cpp's pin from b11960 to b12110 ([#1844](https://github.com/mmogr/gglib/issues/1844)) ([1f75dfe](https://github.com/mmogr/gglib/commit/1f75dfe3b91b7f96adb84297e160524da719f88a))

## [0.32.0-rc.1](https://github.com/mmogr/gglib/compare/v0.31.0-rc.9...v0.32.0-rc.1) (2026-11-09)


### ⚠ BREAKING CHANGES

* **runtime:** the admission paths no chat turn uses are gone, and one token is singular ([#1865](https://github.com/mmogr/gglib/issues/1865))

### Documentation

* **cli:** every snippet is recorded again from the code as it is ([#1862](https://github.com/mmogr/gglib/issues/1862)) ([49e970d](https://github.com/mmogr/gglib/commit/49e970d0c3b91aedf40c41d8204d41515a652a78))


### Internal

* **runtime:** the admission paths no chat turn uses are gone, and one token is singular ([#1865](https://github.com/mmogr/gglib/issues/1865)) ([97b5c27](https://github.com/mmogr/gglib/commit/97b5c27f667e2576467d5c9e0d751c9c629d9a6d))

## [0.31.0-rc.9](https://github.com/mmogr/gglib/compare/v0.31.0-rc.8...v0.31.0-rc.9) (2026-11-09)


### ⚠ BREAKING CHANGES

* **agent:** the old door is gone ([#1858](https://github.com/mmogr/gglib/issues/1858))
* **gui:** sending loads the model; the read-only lock is gone ([#1856](https://github.com/mmogr/gglib/issues/1856))
* **cli:** gglib chat and q are clients of the door ([#1855](https://github.com/mmogr/gglib/issues/1855))
* **agent:** every chat turn goes through the endpoint ([#1854](https://github.com/mmogr/gglib/issues/1854))
* **gui:** the GUI sends a turn, and gglib decides the rest ([#1847](https://github.com/mmogr/gglib/issues/1847))

### What gglib now does

* **agent:** every chat turn goes through the endpoint ([#1854](https://github.com/mmogr/gglib/issues/1854)) ([f76dabb](https://github.com/mmogr/gglib/commit/f76dabb9602c5926181a9fea86dec8d44135badf))
* **cli:** gglib chat and q are clients of the door ([#1855](https://github.com/mmogr/gglib/issues/1855)) ([3f54a78](https://github.com/mmogr/gglib/commit/3f54a7850bc38cdb5c5332df1c1042d5c55d8b46))
* **core:** engine, tool server, download and folder states speak Rust's words ([#1850](https://github.com/mmogr/gglib/issues/1850)) ([b651f69](https://github.com/mmogr/gglib/commit/b651f69beefc51e366ec98e8b3b080c626080e5e))
* **core:** the fit sentence is Rust's, and model explain prints it ([#1848](https://github.com/mmogr/gglib/issues/1848)) ([6ed8c8b](https://github.com/mmogr/gglib/commit/6ed8c8b581b8da66c92e2c4b49daab152efa12f8))
* **gui:** a chat's tools and limits are saved, not kept in the page ([#1853](https://github.com/mmogr/gglib/issues/1853)) ([010c5e2](https://github.com/mmogr/gglib/commit/010c5e2283a06f32dc2ef534e2b8937257cbbb34))
* **gui:** rows answer the arrow keys, and the launch line copies as a command ([#1846](https://github.com/mmogr/gglib/issues/1846)) ([0caa087](https://github.com/mmogr/gglib/commit/0caa087af154159f6382b8983961e0ddf32e3f49))
* **gui:** sending loads the model; the read-only lock is gone ([#1856](https://github.com/mmogr/gglib/issues/1856)) ([ebae746](https://github.com/mmogr/gglib/commit/ebae746cfc85d6853dd48f758b0226d8f95d6b56))
* **gui:** the GUI sends a turn, and gglib decides the rest ([#1847](https://github.com/mmogr/gglib/issues/1847)) ([e1aca48](https://github.com/mmogr/gglib/commit/e1aca48f6128356b64fd95144c05ae27066f56ef))
* **proxy:** the rate and cache figures are Rust's, and the CLI prints them ([#1859](https://github.com/mmogr/gglib/issues/1859)) ([062c495](https://github.com/mmogr/gglib/commit/062c4956c7120fbf96ad1dac0d4a75af85bc254d))


### What it stopped getting wrong

* **models:** verify and the inspector say one verdict, from Rust ([#1849](https://github.com/mmogr/gglib/issues/1849)) ([d1bae58](https://github.com/mmogr/gglib/commit/d1bae586023b11be460fe60fb650ba4f6f0fef2b))
* **proxy:** a pinned endpoint names its model by id, and every match on the pin uses it ([#1857](https://github.com/mmogr/gglib/issues/1857)) ([7a2338d](https://github.com/mmogr/gglib/commit/7a2338dc4b81084eb1efeb90708415c2e67c4ee5))


### Internal

* **agent:** the old door is gone ([#1858](https://github.com/mmogr/gglib/issues/1858)) ([3722252](https://github.com/mmogr/gglib/commit/372225223fc2f4db553fcbca7e3f832d57a947b3))

## [0.31.0-rc.8](https://github.com/mmogr/gglib/compare/v0.31.0-rc.7...v0.31.0-rc.8) (2026-11-08)


### What it stopped getting wrong

* **chats:** a model is found by its id, and one this machine does not hold says so ([#1841](https://github.com/mmogr/gglib/issues/1841)) ([1b44152](https://github.com/mmogr/gglib/commit/1b44152d44edfad14d5a21187bcde03fb9233594))
* **gui:** no more overlapping text in the inspector, a library that fits a narrow window, and no wordmark under the Mac window buttons ([#1840](https://github.com/mmogr/gglib/issues/1840)) ([30697f7](https://github.com/mmogr/gglib/commit/30697f7663edce03720f8bff8ef0eb16af7ea08e))

## [0.31.0-rc.7](https://github.com/mmogr/gglib/compare/v0.31.0-rc.6...v0.31.0-rc.7) (2026-11-08)


### What gglib now does

* **agent:** the turn handler serves this machine's chats too ([#1835](https://github.com/mmogr/gglib/issues/1835)) ([d056e59](https://github.com/mmogr/gglib/commit/d056e59d5ba52f36cd5484e7d6c5aade0eeb38f4))
* **app-services:** local and paired chat dial one door ([#1825](https://github.com/mmogr/gglib/issues/1825)) ([a3c6cd1](https://github.com/mmogr/gglib/commit/a3c6cd14fd1e0acdfe8be1ddbfddfadd621d3ac2))
* **chats:** a conversation's model, tools and limits can be set over HTTP ([#1827](https://github.com/mmogr/gglib/issues/1827)) ([b6f1a0b](https://github.com/mmogr/gglib/commit/b6f1a0bc5fe6f6e45b82df1d897265903c670c20))
* **gui:** a redrawn app icon that macOS 26 shows full size, and a tray icon from it ([#1837](https://github.com/mmogr/gglib/issues/1837)) ([9449d55](https://github.com/mmogr/gglib/commit/9449d55548022c297b564af5b83de6b26b4da9fe))
* **gui:** one way to say a load failed or a copy worked, and the picker says what is loaded ([#1836](https://github.com/mmogr/gglib/issues/1836)) ([cd69255](https://github.com/mmogr/gglib/commit/cd69255e7fb01d4cd976537b38d6c26c1fad981c))
* **gui:** the endpoint dot blinks per request, and Diagnostics opens on Live from the menu too ([#1824](https://github.com/mmogr/gglib/issues/1824)) ([dc14362](https://github.com/mmogr/gglib/commit/dc143629d98ffea09ad87b80b84e8aa062b31236))

## [0.31.0-rc.6](https://github.com/mmogr/gglib/compare/v0.31.0-rc.5...v0.31.0-rc.6) (2026-11-07)


### What gglib now does

* **gui:** the tray's Start gglib starts gglib ([#1819](https://github.com/mmogr/gglib/issues/1819)) ([a3449ac](https://github.com/mmogr/gglib/commit/a3449ac03d78fb5ecf1592093e1bf19778269261))


### Documentation

* the screenshots and prose name the new screens ([#1822](https://github.com/mmogr/gglib/issues/1822)) ([651ff25](https://github.com/mmogr/gglib/commit/651ff2574a47af91416ec9d40074fb7f6fb2eeb7))


### Internal

* **gui:** setup's build poll is counted after its cleanup has run ([#1820](https://github.com/mmogr/gglib/issues/1820)) ([50640c9](https://github.com/mmogr/gglib/commit/50640c90e2c12050581479875d9ba4780754045b))
* **release:** the release PR's numbers open their pages ([#1823](https://github.com/mmogr/gglib/issues/1823)) ([ee45671](https://github.com/mmogr/gglib/commit/ee4567121adc3435e6224176c818f06af616c05e))

## [0.31.0-rc.5](https://github.com/mmogr/gglib/compare/v0.31.0-rc.4...v0.31.0-rc.5) (2026-11-07)


### ⚠ BREAKING CHANGES

* **gui:** tool servers are rows with one sheet to add them ([#1818](https://github.com/mmogr/gglib/issues/1818))
* **tauri:** the app and tray menus speak the new vocabulary ([#1813](https://github.com/mmogr/gglib/issues/1813))
* **gui:** engines are rows, and About can stop gglib ([#1814](https://github.com/mmogr/gglib/issues/1814))
* **gui:** diagnostics holds the live numbers, logs, benchmarks and system checks ([#1812](https://github.com/mmogr/gglib/issues/1812))
* **gui:** one sheet views, fixes and redraws a picture ([#1808](https://github.com/mmogr/gglib/issues/1808))
* **gui:** a reply folds its thinking and tool calls, and shows its figures on hover ([#1807](https://github.com/mmogr/gglib/issues/1807))
* **gui:** the composer has one + menu, chips only when set, and a labelled Thinking toggle ([#1806](https://github.com/mmogr/gglib/issues/1806))
* **gui:** setup is five steps that end with a proven endpoint ([#1811](https://github.com/mmogr/gglib/issues/1811))
* **gui:** settings is a place with six sections that save as you go ([#1805](https://github.com/mmogr/gglib/issues/1805))
* **gui:** the tray shows the endpoint, what is loaded, and devices ([#1804](https://github.com/mmogr/gglib/issues/1804))
* **gui:** chats live in the sidebar and a chat is a place ([#1802](https://github.com/mmogr/gglib/issues/1802))

### What gglib now does

* **gui:** a reply folds its thinking and tool calls, and shows its figures on hover ([#1807](https://github.com/mmogr/gglib/issues/1807)) ([e03faf0](https://github.com/mmogr/gglib/commit/e03faf0c0924c671b480a11866bf347c1ecac778))
* **gui:** chats live in the sidebar and a chat is a place ([#1802](https://github.com/mmogr/gglib/issues/1802)) ([fffb8b0](https://github.com/mmogr/gglib/commit/fffb8b003cfa739e48d983f0f2cfad6efe7387c5))
* **gui:** diagnostics holds the live numbers, logs, benchmarks and system checks ([#1812](https://github.com/mmogr/gglib/issues/1812)) ([b2ce3a5](https://github.com/mmogr/gglib/commit/b2ce3a5e3804467ca3a8742dbbe01d3ef7085d65))
* **gui:** engines are rows, and About can stop gglib ([#1814](https://github.com/mmogr/gglib/issues/1814)) ([0fc71f2](https://github.com/mmogr/gglib/commit/0fc71f2c44f056e5e07328241d8e9c0b1c1f40e1))
* **gui:** one sheet views, fixes and redraws a picture ([#1808](https://github.com/mmogr/gglib/issues/1808)) ([09db379](https://github.com/mmogr/gglib/commit/09db379f002c936e642ea4b29b09ffa42fd73311))
* **gui:** settings is a place with six sections that save as you go ([#1805](https://github.com/mmogr/gglib/issues/1805)) ([c198781](https://github.com/mmogr/gglib/commit/c198781d3f387bb92c7fd4511f923ba04d675850))
* **gui:** setup is five steps that end with a proven endpoint ([#1811](https://github.com/mmogr/gglib/issues/1811)) ([4014809](https://github.com/mmogr/gglib/commit/4014809ef5375b3e7acb0db738c2fbbc79c9a734))
* **gui:** the composer has one + menu, chips only when set, and a labelled Thinking toggle ([#1806](https://github.com/mmogr/gglib/issues/1806)) ([54e812d](https://github.com/mmogr/gglib/commit/54e812de274ffcf48f1f6df100f689df58d7a109))
* **gui:** the tray shows the endpoint, what is loaded, and devices ([#1804](https://github.com/mmogr/gglib/issues/1804)) ([5aaa243](https://github.com/mmogr/gglib/commit/5aaa243eee9e947bd9ad5cc5c51b086b059976f2))
* **gui:** tool servers are rows with one sheet to add them ([#1818](https://github.com/mmogr/gglib/issues/1818)) ([2be2267](https://github.com/mmogr/gglib/commit/2be226786e5d5f5ff4b713a349ce7288ba0397ac))
* **tauri:** the app and tray menus speak the new vocabulary ([#1813](https://github.com/mmogr/gglib/issues/1813)) ([43ed21e](https://github.com/mmogr/gglib/commit/43ed21e73667f44e75cf1996eff900e4dcf4575a))


### What it stopped getting wrong

* **cli:** gglib model explain says what a load would unload ([#1815](https://github.com/mmogr/gglib/issues/1815)) ([539d871](https://github.com/mmogr/gglib/commit/539d87114556b2c3754fd2dfa0bf25d0abbc2a61)), closes [#1809](https://github.com/mmogr/gglib/issues/1809)
* **downloads:** the download queue size is read when the daemon starts, and a CLI write reaches it ([#1816](https://github.com/mmogr/gglib/issues/1816)) ([414f3dc](https://github.com/mmogr/gglib/commit/414f3dc83d5ae8dca5f4d3bf3b84ff280f0b3a9a)), closes [#1810](https://github.com/mmogr/gglib/issues/1810)

## [0.31.0-rc.4](https://github.com/mmogr/gglib/compare/v0.31.0-rc.3...v0.31.0-rc.4) (2026-11-07)


### ⚠ BREAKING CHANGES

* **gui:** devices have a page: share this Mac, paired devices, use another machine ([#1800](https://github.com/mmogr/gglib/issues/1800))
* **gui:** the endpoint has a page: its address, what is loaded and how, and who uses it ([#1798](https://github.com/mmogr/gglib/issues/1798))
* **cli:** a piped "y" (`yes | gglib serve …`) no longer accepts the install; use a terminal, `gglib up --yes`, or `gglib config llama install`.

### What gglib now does

* **gui:** devices have a page: share this Mac, paired devices, use another machine ([#1800](https://github.com/mmogr/gglib/issues/1800)) ([9dd7d3c](https://github.com/mmogr/gglib/commit/9dd7d3ceebbefa6a524f067e21d3611b6a26b711))
* **gui:** Load a model… is a palette on the endpoint page ([#1799](https://github.com/mmogr/gglib/issues/1799)) ([4c5fdce](https://github.com/mmogr/gglib/commit/4c5fdce3ed21f267ba3d1f923e69d025a1d1d752))
* **gui:** the endpoint has a page: its address, what is loaded and how, and who uses it ([#1798](https://github.com/mmogr/gglib/issues/1798)) ([e876a06](https://github.com/mmogr/gglib/commit/e876a06732d7134080975d0054999e75e065c48e))


### What it stopped getting wrong

* **cli:** gglib model load asks about an engine update only on a terminal ([#1796](https://github.com/mmogr/gglib/issues/1796)) ([531c23d](https://github.com/mmogr/gglib/commit/531c23d96076c3b377bd2a390a263b8f42fa8ef0))

## [0.31.0-rc.3](https://github.com/mmogr/gglib/compare/v0.31.0-rc.2...v0.31.0-rc.3) (2026-11-07)


### ⚠ BREAKING CHANGES

* **gui:** the inspector also holds a far model, a Hugging Face repo, and a loaded model's log ([#1795](https://github.com/mmogr/gglib/issues/1795))
* **gui:** verify and update results appear in Details ([#1794](https://github.com/mmogr/gglib/issues/1794))
* **gui:** Load with options is a sheet, and the Serve modal is gone ([#1792](https://github.com/mmogr/gglib/issues/1792))
* **gui:** the inspector says how a model runs and keeps the rest under Details ([#1791](https://github.com/mmogr/gglib/issues/1791))

### What gglib now does

* **gui:** Load with options is a sheet, and the Serve modal is gone ([#1792](https://github.com/mmogr/gglib/issues/1792)) ([cf9a3a8](https://github.com/mmogr/gglib/commit/cf9a3a89f68e06a7ce9f05a65945c09a1676eadb))
* **gui:** the inspector also holds a far model, a Hugging Face repo, and a loaded model's log ([#1795](https://github.com/mmogr/gglib/issues/1795)) ([3988ea7](https://github.com/mmogr/gglib/commit/3988ea70f1766038a442480dc8736b70376368e0))
* **gui:** the inspector says how a model runs and keeps the rest under Details ([#1791](https://github.com/mmogr/gglib/issues/1791)) ([a0d720f](https://github.com/mmogr/gglib/commit/a0d720f7fb4ce6e15107e39413683953edd96474))
* **gui:** verify and update results appear in Details ([#1794](https://github.com/mmogr/gglib/issues/1794)) ([a70c815](https://github.com/mmogr/gglib/commit/a70c8158caeff6680976cf03a13ec3dcc7964292))

## [0.31.0-rc.2](https://github.com/mmogr/gglib/compare/v0.31.0-rc.1...v0.31.0-rc.2) (2026-11-07)


### ⚠ BREAKING CHANGES

* **gui:** one search finds your models, then Hugging Face on Enter ([#1790](https://github.com/mmogr/gglib/issues/1790))
* **gui:** the library is groups of rows, and downloads are rows ([#1789](https://github.com/mmogr/gglib/issues/1789))

### What gglib now does

* **gui:** one search finds your models, then Hugging Face on Enter ([#1790](https://github.com/mmogr/gglib/issues/1790)) ([8705103](https://github.com/mmogr/gglib/commit/87051038688fdfc4d3481212f923447bdc407827))
* **gui:** the library is groups of rows, and downloads are rows ([#1789](https://github.com/mmogr/gglib/issues/1789)) ([aa925e3](https://github.com/mmogr/gglib/commit/aa925e36a3a73d91fb72215b57bf4e38f8efbb6c))
* **models:** every library model says its size on disk ([#1788](https://github.com/mmogr/gglib/issues/1788)) ([839c4a0](https://github.com/mmogr/gglib/commit/839c4a0b1d48fdd14ad2264d674c67af8d3dda9f))


### What it stopped getting wrong

* **app-services:** the pin and the image refusal are tested, and each key fn keeps its own doc ([#1786](https://github.com/mmogr/gglib/issues/1786)) ([0b18efb](https://github.com/mmogr/gglib/commit/0b18efbe18bebd8c8cddcd4db791cb4f4c11488f))


### Internal

* **contracts:** the committed recording names the help's flags ([#1782](https://github.com/mmogr/gglib/issues/1782)) ([3b21ffd](https://github.com/mmogr/gglib/commit/3b21ffd98c54ab3b73635d9a5b269efa4ab7c160))
* **deps:** update huggingface-hub requirement ([#1754](https://github.com/mmogr/gglib/issues/1754)) ([87c5d62](https://github.com/mmogr/gglib/commit/87c5d62bf0ce72ee750fce62538992af1e285dd8))
* **gate:** CI Success names the job a skip waited on, and says a cancel can be a timeout ([#1783](https://github.com/mmogr/gglib/issues/1783)) ([3904009](https://github.com/mmogr/gglib/commit/3904009dde3a17613c0034cdb33703b1d9296a6c))
* **prelude:** apt fetches from archive.ubuntu.com when the Azure mirror is slow ([#1781](https://github.com/mmogr/gglib/issues/1781)) ([ecac476](https://github.com/mmogr/gglib/commit/ecac476e2c880f46e1bb1de23fe782ed2c3eeda8))

## [0.31.0-rc.1](https://github.com/mmogr/gglib/compare/v0.30.1-rc.1...v0.31.0-rc.1) (2026-11-06)


### ⚠ BREAKING CHANGES

* **app-services:** client recipes and the proof request belong to the daemon ([#1779](https://github.com/mmogr/gglib/issues/1779))
* **gui:** a sidebar names the library, the endpoint and devices; the header is gone ([#1777](https://github.com/mmogr/gglib/issues/1777))
* **models:** templates and speculative decoding are a model's own defaults ([#1764](https://github.com/mmogr/gglib/issues/1764))
* **app-services:** model explain says the launch it would make: argv, each flag's source, and the fit ([#1762](https://github.com/mmogr/gglib/issues/1762))
* **gui:** the main window's screens are addresses; `gglib web`'s link and a reload land on the place the address names, and the desktop app's asset paths are absolute.
* **settings:** `gglib proxy --host` and `gglib serve --host` default to the stored `proxy_host` (loopback until set) rather than always to loopback; the desktop app's per-run connection options are removed in favour of the settings.
* **gui:** the accent is ink; amber means attention ([#1751](https://github.com/mmogr/gglib/issues/1751))
* **gui:** four type sizes, status hues apart, nothing pulses ([#1750](https://github.com/mmogr/gglib/issues/1750))
* **runtime:** every load and unload is announced, whoever caused it ([#1761](https://github.com/mmogr/gglib/issues/1761))
* **hf:** every variant says whether it fits this machine ([#1765](https://github.com/mmogr/gglib/issues/1765))
* **proxy:** `GET /api/proxy/status` (and a start's or stop's answer) no longer carries `current_model` or `model_port`.
* **core:** the state words are written once ([#1759](https://github.com/mmogr/gglib/issues/1759))

### What gglib now does

* **app-services:** client recipes and the proof request belong to the daemon ([#1779](https://github.com/mmogr/gglib/issues/1779)) ([694fc0b](https://github.com/mmogr/gglib/commit/694fc0b5ae1f9101745b969bc3717e618e0deeb5))
* **app-services:** model explain says the launch it would make: argv, each flag's source, and the fit ([#1762](https://github.com/mmogr/gglib/issues/1762)) ([80b4115](https://github.com/mmogr/gglib/commit/80b4115baf222bc8f30b33be7af80ced45db1481))
* **cli:** gglib model load and unload ([#1769](https://github.com/mmogr/gglib/issues/1769)) ([8f0f4ad](https://github.com/mmogr/gglib/commit/8f0f4ad7fe3d9007d3ce26519d99698fde94ae76))
* **core:** the state words are written once ([#1759](https://github.com/mmogr/gglib/issues/1759)) ([7e394f5](https://github.com/mmogr/gglib/commit/7e394f5052b515c514a4e89ab4a333fae5e19d9b))
* **gui:** a sidebar names the library, the endpoint and devices; the header is gone ([#1777](https://github.com/mmogr/gglib/issues/1777)) ([48f57c5](https://github.com/mmogr/gglib/commit/48f57c58df3453c257e999b46473085f2a6f366f))
* **gui:** every screen has a URL ([#1774](https://github.com/mmogr/gglib/issues/1774)) ([3ff4c28](https://github.com/mmogr/gglib/commit/3ff4c2831ca94d8d8b1f8be49ee056394b293cb1))
* **gui:** four type sizes, status hues apart, nothing pulses ([#1750](https://github.com/mmogr/gglib/issues/1750)) ([3859ad7](https://github.com/mmogr/gglib/commit/3859ad72505b85b9a9b865f6a2ad5d92f2f74522))
* **gui:** the accent is ink; amber means attention ([#1751](https://github.com/mmogr/gglib/issues/1751)) ([78835b1](https://github.com/mmogr/gglib/commit/78835b148e843d3650dbe27fb0f90514861b1133))
* **hf:** every variant says whether it fits this machine ([#1765](https://github.com/mmogr/gglib/issues/1765)) ([f763819](https://github.com/mmogr/gglib/commit/f76381904e7f8d02e3fbdef580898b08198fa5b6))
* **models:** templates and speculative decoding are a model's own defaults ([#1764](https://github.com/mmogr/gglib/issues/1764)) ([75c69e1](https://github.com/mmogr/gglib/commit/75c69e17a9d5e8589b4db568c011d55b51a4f889))
* **models:** the id a client sends is stated once, and a ref is shown whole ([#1766](https://github.com/mmogr/gglib/issues/1766)) ([2ad6804](https://github.com/mmogr/gglib/commit/2ad68045b2a814bd247275ae9fc277f1c99ea3d9))
* **proxy:** each loaded model says how it was launched and what it is doing; the second slot says how much room is left ([#1778](https://github.com/mmogr/gglib/issues/1778)) ([9aab47f](https://github.com/mmogr/gglib/commit/9aab47f5fe21c2c377821195e6f454cd8f93cb22))
* **proxy:** proxy status says its address and whether it needs a key ([#1768](https://github.com/mmogr/gglib/issues/1768)) ([e56dc55](https://github.com/mmogr/gglib/commit/e56dc55a935d7b439b40fa82fc67b13d75011002))
* **proxy:** the request ledger says what gglib did to each request ([#1770](https://github.com/mmogr/gglib/issues/1770)) ([46899db](https://github.com/mmogr/gglib/commit/46899dbeeb2630a09e80929ec009056a1006b651))
* **settings:** the endpoint's bind address, allowed hosts and on-disk prompt cache are settings ([#1771](https://github.com/mmogr/gglib/issues/1771)) ([661cf47](https://github.com/mmogr/gglib/commit/661cf47408371436c30195bcf5199933477aa0ea))


### What it stopped getting wrong

* **runtime:** every load and unload is announced, whoever caused it ([#1761](https://github.com/mmogr/gglib/issues/1761)) ([28ec436](https://github.com/mmogr/gglib/commit/28ec436a05cedab170af148d37a4b8a700eb2291))


### Internal

* **deps:** bump @vitejs/plugin-react ([#1756](https://github.com/mmogr/gglib/issues/1756)) ([5d5cf45](https://github.com/mmogr/gglib/commit/5d5cf45be26f84e750a74d69beae2ed6996cfbc8))
* **deps:** bump globals from 17.13.0 to 17.14.0 in the eslint group ([#1755](https://github.com/mmogr/gglib/issues/1755)) ([1cd9f3a](https://github.com/mmogr/gglib/commit/1cd9f3a3b0a747d9b07980544f3255f20a5f85eb))
* **deps:** bump knip from 6.44.0 to 6.44.1 in the npm-compatible group ([#1745](https://github.com/mmogr/gglib/issues/1745)) ([c4455d1](https://github.com/mmogr/gglib/commit/c4455d1098393ca7f49308a5229c8424f5e942f5))
* **deps:** bump taiki-e/install-action ([#1757](https://github.com/mmogr/gglib/issues/1757)) ([e190fb4](https://github.com/mmogr/gglib/commit/e190fb4adc5cfc2f2e2b357157357984e86d85ee))
* **deps:** bump the cargo-compatible group with 6 updates ([#1758](https://github.com/mmogr/gglib/issues/1758)) ([f4bb333](https://github.com/mmogr/gglib/commit/f4bb33353827e7aec1e827e3f65ac72690be1954))
* **deps:** update hf-xet requirement in /crates/gglib-download/scripts ([#1752](https://github.com/mmogr/gglib/issues/1752)) ([536da86](https://github.com/mmogr/gglib/commit/536da86afb874af5bb7d574e5ccbb705473cae2e))
* **gui:** one model registry and one dashboard stream ([#1773](https://github.com/mmogr/gglib/issues/1773)) ([cbd3abe](https://github.com/mmogr/gglib/commit/cbd3abeefdf32acbe8948b39b8963af5b1f2f796))
* **gui:** the readers the contracts pin live in src/readings ([#1772](https://github.com/mmogr/gglib/issues/1772)) ([52f5925](https://github.com/mmogr/gglib/commit/52f5925d9772d29656f3d8bb25f3905875a709cf))
* **release:** the release PR proposes the right candidate without a Release-As line ([#1748](https://github.com/mmogr/gglib/issues/1748)) ([023b0a3](https://github.com/mmogr/gglib/commit/023b0a30ae34d83306d414fd2a64ad960d3d080f)), closes [#1747](https://github.com/mmogr/gglib/issues/1747)

## [0.30.1-rc.1](https://github.com/mmogr/gglib/compare/v0.30.0-rc.3...v0.30.1-rc.1) (2026-11-03)


### What it stopped getting wrong

* **release:** the Fedora install check disables the Cisco repository by id ([#1742](https://github.com/mmogr/gglib/issues/1742)) ([ec67898](https://github.com/mmogr/gglib/commit/ec67898624fd4de504a6b8e5e5f32176f2a986ae))

## [0.30.0-rc.3](https://github.com/mmogr/gglib/compare/v0.30.0-rc.2...v0.30.0-rc.3) (2026-11-02)


### ⚠ BREAKING CHANGES

* **core:** `gglib_core::Quantization`, `gglib_core::download::Quantization`, `gglib_download::Quantization` and `DownloadError::InvalidQuantization` are removed.
* **core:** a file's role is never read from its name ([#1737](https://github.com/mmogr/gglib/issues/1737))
* **download:** with no quant asked, the largest quantized variant that fits ([#1736](https://github.com/mmogr/gglib/issues/1736))
* **hf:** a repository's variants are shard sets named as their uploader named them ([#1735](https://github.com/mmogr/gglib/issues/1735))
* **db:** models store the header's type and bits per weight ([#1734](https://github.com/mmogr/gglib/issues/1734))

### What gglib now does

* **contracts:** the help reading names the flags ([#1725](https://github.com/mmogr/gglib/issues/1725)) ([14473ff](https://github.com/mmogr/gglib/commit/14473ff7adb846d6caf1f51762aed67d7114554f))
* **db:** models store the header's type and bits per weight ([#1734](https://github.com/mmogr/gglib/issues/1734)) ([cd83837](https://github.com/mmogr/gglib/commit/cd83837c51611f4da21fb6c19bef6c695f230175))
* **download:** with no quant asked, the largest quantized variant that fits ([#1736](https://github.com/mmogr/gglib/issues/1736)) ([630ebd5](https://github.com/mmogr/gglib/commit/630ebd5ef37e2bae7e0d85bcdbfa54619bd526b6))
* **engine-pins:** the pin body reads upstream against gglib's own features ([#1728](https://github.com/mmogr/gglib/issues/1728)) ([a52304e](https://github.com/mmogr/gglib/commit/a52304e6c1fc127fe6533f5fc8a2805810b5860c))
* **hf:** a repository's variants are shard sets named as their uploader named them ([#1735](https://github.com/mmogr/gglib/issues/1735)) ([e39af94](https://github.com/mmogr/gglib/commit/e39af94355cadfbfb9e8eb760c159676de203861))
* **images:** a model is drawn by its variant, read from its file name ([#1717](https://github.com/mmogr/gglib/issues/1717)) ([5155326](https://github.com/mmogr/gglib/commit/5155326caffc249bf8ade1fdde62e151002c4e67))
* **models:** gglib knows FLUX.2 klein ([#1720](https://github.com/mmogr/gglib/issues/1720)) ([4956942](https://github.com/mmogr/gglib/commit/49569429e864efe9ae2b1442f85b77ec59e977ff))
* **models:** gglib knows FLUX.3 Image Klein ([#1718](https://github.com/mmogr/gglib/issues/1718)) ([050c19b](https://github.com/mmogr/gglib/commit/050c19b48db1237c35d98f78f2e5488b2771fe9c))


### What it stopped getting wrong

* **engine-pins:** the pin body lists stable-diffusion.cpp's server commits ([#1724](https://github.com/mmogr/gglib/issues/1724)) ([6cded37](https://github.com/mmogr/gglib/commit/6cded37e5bed7ad9ba06e9f7e8a29215ae1d1c76)), closes [#1722](https://github.com/mmogr/gglib/issues/1722)
* **gguf:** what a file is comes from its header, never its name ([#1731](https://github.com/mmogr/gglib/issues/1731)) ([f8d3de7](https://github.com/mmogr/gglib/commit/f8d3de7ea6bf3a40ae3266d33166e88d28be760d))
* **quant:** the follow-ups the quant stack's reviews left ([#1741](https://github.com/mmogr/gglib/issues/1741)) ([cc34c9c](https://github.com/mmogr/gglib/commit/cc34c9c5a39dde4b42f6ca7bad43e3e1e615f638))


### Documentation

* **models:** name models that come with a projector ([#1716](https://github.com/mmogr/gglib/issues/1716)) ([1675a79](https://github.com/mmogr/gglib/commit/1675a79632bbdfa1c8c17f7c7b443948a55f619b))
* **readme:** the lead says what gglib is before what it does ([#1730](https://github.com/mmogr/gglib/issues/1730)) ([18f43a6](https://github.com/mmogr/gglib/commit/18f43a6c6e4251d2ff82c1ae2100a22c97d78b94))


### Internal

* **core:** a file's role is never read from its name ([#1737](https://github.com/mmogr/gglib/issues/1737)) ([cfc35c1](https://github.com/mmogr/gglib/commit/cfc35c1240b0a1e830065644c4f88a290426b692))
* **core:** the closed quantization enum is gone ([#1738](https://github.com/mmogr/gglib/issues/1738)) ([9ccb5fe](https://github.com/mmogr/gglib/commit/9ccb5fe683130bdcdd2af82dd7d809147c9b244d))
* **deps:** move llama.cpp's pin from b11669 to b11960 ([#1715](https://github.com/mmogr/gglib/issues/1715)) ([e990d9c](https://github.com/mmogr/gglib/commit/e990d9c9b2b8a8bc6c9810d598bf65863cf07954))
* **deps:** move stable-diffusion.cpp's pin from master-1000-95ff762 to master-1025-8ee08e3 ([#1714](https://github.com/mmogr/gglib/issues/1714)) ([2394bd4](https://github.com/mmogr/gglib/commit/2394bd467ca49944dc6762f2246783a5b65ffae9))
* **engine-pins:** a /approve comment on the pull request writes the approval ([#1723](https://github.com/mmogr/gglib/issues/1723)) ([f324875](https://github.com/mmogr/gglib/commit/f3248750a8b32bbae630fb3f9e413ac320bcd165))
* **proxy:** the objects gglib reads from llama-server are held to the recording ([#1726](https://github.com/mmogr/gglib/issues/1726)) ([37736f0](https://github.com/mmogr/gglib/commit/37736f03869100a4337dea1ad25702e8b27673b1))
* **release:** cut a maintenance line and backport to it by label ([#1733](https://github.com/mmogr/gglib/issues/1733)) ([4d0f168](https://github.com/mmogr/gglib/commit/4d0f1683e3402cae861d5c9e962843818f037185))
* **release:** maintenance lines release/X.Y.x carry a stable version's patches ([#1732](https://github.com/mmogr/gglib/issues/1732)) ([8b49e12](https://github.com/mmogr/gglib/commit/8b49e12d7363b6f804ee50621ed8bfc6f4f80f8e))
* the check's comment reads top down ([#1729](https://github.com/mmogr/gglib/issues/1729)) ([309864b](https://github.com/mmogr/gglib/commit/309864b2dd8db763511c33cdba7affebb8fb0bc1))

## [0.30.0-rc.2](https://github.com/mmogr/gglib/compare/v0.30.0-rc.1...v0.30.0-rc.2) (2026-11-01)


### ⚠ BREAKING CHANGES

* **cli:** `gglib --remote <command>` no longer parses; the flag goes after the command it is for.

### What gglib now does

* **cli:** --remote is offered only by the commands that reach the paired machine ([#1709](https://github.com/mmogr/gglib/issues/1709)) ([dcfba8b](https://github.com/mmogr/gglib/commit/dcfba8b56ec04ac00cb5c59a30fb82e9008e71c6))


### What it stopped getting wrong

* **cli:** gglib image --remote draws with the paired machine's image model ([#1711](https://github.com/mmogr/gglib/issues/1711)) ([728ee36](https://github.com/mmogr/gglib/commit/728ee3618df814a52f46300d569e28b523addc23)), closes [#1707](https://github.com/mmogr/gglib/issues/1707)


### Internal

* **cli:** every route the tunnel carries is reached by a command or exempted ([#1708](https://github.com/mmogr/gglib/issues/1708)) ([5727676](https://github.com/mmogr/gglib/commit/5727676e9e7c2da1058f2d61eaee90f896123af1))
* **cli:** the CLI reaches the paired machine through FarProxy ([#1710](https://github.com/mmogr/gglib/issues/1710)) ([8eaf859](https://github.com/mmogr/gglib/commit/8eaf85946658361c2469be9253343257d43bff5b))

## [0.30.0-rc.1](https://github.com/mmogr/gglib/compare/v0.29.0...v0.30.0-rc.1) (2026-11-01)


### ⚠ BREAKING CHANGES

* **mcp:** the manual lifecycle is gone ([#1704](https://github.com/mmogr/gglib/issues/1704))
* **mcp:** tools and list ask the daemon, and start and stop are gone ([#1703](https://github.com/mmogr/gglib/issues/1703))

### What it stopped getting wrong

* **benchmark:** compare counts its tokens and keeps the thinking apart ([#1702](https://github.com/mmogr/gglib/issues/1702)) ([2999e9c](https://github.com/mmogr/gglib/commit/2999e9c371cc03961e6d075315b4be1736454254)), closes [#1690](https://github.com/mmogr/gglib/issues/1690)
* **mcp:** tools and list ask the daemon, and start and stop are gone ([#1703](https://github.com/mmogr/gglib/issues/1703)) ([318a022](https://github.com/mmogr/gglib/commit/318a0223d31398de64482783d441a9484cad16c1)), closes [#1689](https://github.com/mmogr/gglib/issues/1689)
* **proxy:** two models with one name get two ids ([#1705](https://github.com/mmogr/gglib/issues/1705)) ([be96abb](https://github.com/mmogr/gglib/commit/be96abb2d0af6c51dc7af605e3edd93d70d941b8)), closes [#1648](https://github.com/mmogr/gglib/issues/1648)
* **remote:** disable lets go of the serve slot before it drains ([#1701](https://github.com/mmogr/gglib/issues/1701)) ([514f504](https://github.com/mmogr/gglib/commit/514f5041d53e4750fc432c3ddc4b30ab28a239d4)), closes [#1646](https://github.com/mmogr/gglib/issues/1646)
* **runtime:** a pid that names a process group is never signalled ([#1700](https://github.com/mmogr/gglib/issues/1700)) ([6ac773e](https://github.com/mmogr/gglib/commit/6ac773e7dc8ff46ecb8d939d8fca45fa4b265acc)), closes [#1647](https://github.com/mmogr/gglib/issues/1647)


### Internal

* **mcp:** the manual lifecycle is gone ([#1704](https://github.com/mmogr/gglib/issues/1704)) ([11208b8](https://github.com/mmogr/gglib/commit/11208b84617d793b07bb164b4b1de9b086453077)), closes [#1699](https://github.com/mmogr/gglib/issues/1699)
* **promote:** the stable release is made from the candidate's tag, not from main ([#1697](https://github.com/mmogr/gglib/issues/1697)) ([48303e8](https://github.com/mmogr/gglib/commit/48303e814905739ea04ee4e0bab4b9a24862814d))

## [0.29.0](https://github.com/mmogr/gglib/compare/v0.28.0...v0.29.0) (2026-10-31)

0.29.0-rc.7, promoted unchanged. The candidates below make up this release.

## [0.29.0-rc.7](https://github.com/mmogr/gglib/compare/v0.29.0-rc.6...v0.29.0-rc.7) (2026-10-31)


### ⚠ BREAKING CHANGES

* **cli:** -y/--yes for a flag that only answers the confirmation, and no global --models-dir ([#1693](https://github.com/mmogr/gglib/issues/1693))

### What it stopped getting wrong

* **daemon:** a stop waits until the daemon has let go of its lock ([#1691](https://github.com/mmogr/gglib/issues/1691)) ([7898b3d](https://github.com/mmogr/gglib/commit/7898b3d64323714d6adc7582e5bfa81bfdac2b61))


### Documentation

* **cli:** ROCm in backticks, as clippy's doc_markdown asks ([#1695](https://github.com/mmogr/gglib/issues/1695)) ([cef3f28](https://github.com/mmogr/gglib/commit/cef3f280063cbed083b827719e17f3d4c87582a9))
* the architecture diagrams and tables match the code ([#1692](https://github.com/mmogr/gglib/issues/1692)) ([039e7a6](https://github.com/mmogr/gglib/commit/039e7a6ebd1d3d8e154a8dcb7702bb6ea0f46583))


### Internal

* **cli:** -y/--yes for a flag that only answers the confirmation, and no global --models-dir ([#1693](https://github.com/mmogr/gglib/issues/1693)) ([8c8ec2d](https://github.com/mmogr/gglib/commit/8c8ec2d5bad68df9dd9cef78e487a4652520afb7))

## [0.29.0-rc.6](https://github.com/mmogr/gglib/compare/v0.29.0-rc.5...v0.29.0-rc.6) (2026-10-31)


### What it stopped getting wrong

* **ci:** the tag is made with the App's token on every path ([#1686](https://github.com/mmogr/gglib/issues/1686)) ([503d563](https://github.com/mmogr/gglib/commit/503d56334c4ae4f39c606467b1372777b4847bba)), closes [#1685](https://github.com/mmogr/gglib/issues/1685)
* **ci:** the tagging workflow run by hand tags with the App's token ([#1684](https://github.com/mmogr/gglib/issues/1684)) ([f2604e2](https://github.com/mmogr/gglib/commit/f2604e2bf4b00b15ea1d37d9cd5ae16ff7ce7108)), closes [#1683](https://github.com/mmogr/gglib/issues/1683)


### Internal

* **release:** the tagging workflow runs by hand, without waiting for CI ([#1682](https://github.com/mmogr/gglib/issues/1682)) ([7d44a34](https://github.com/mmogr/gglib/commit/7d44a34536ce0d98fd33e959db5cf22d2e08b848))

## [0.29.0-rc.5](https://github.com/mmogr/gglib/compare/v0.29.0-rc.4...v0.29.0-rc.5) (2026-10-31)


### What it stopped getting wrong

* **ci:** the release job can relabel the release PR ([#1681](https://github.com/mmogr/gglib/issues/1681)) ([12c4430](https://github.com/mmogr/gglib/commit/12c44308c2539621f757ebfb4d5cf7a4c9d8b6ce)), closes [#1679](https://github.com/mmogr/gglib/issues/1679)


### Documentation

* **readme:** one badge for the stable release, one for the release candidate ([#1678](https://github.com/mmogr/gglib/issues/1678)) ([240a8fd](https://github.com/mmogr/gglib/commit/240a8fdded555b64a6723658c100a8f210622d89))
* **readme:** the badges say where gglib installs from and how each workflow stands ([#1677](https://github.com/mmogr/gglib/issues/1677)) ([7328f74](https://github.com/mmogr/gglib/commit/7328f74e91af2e8791b88b07ca2e48ef7a8501ac))

## [0.29.0-rc.4](https://github.com/mmogr/gglib/compare/v0.29.0-rc.3...v0.29.0-rc.4) (2026-10-31)


### What gglib now does

* **packaging:** pacman and dnf repositories, and one install command for every distro ([#1669](https://github.com/mmogr/gglib/issues/1669)) ([253a523](https://github.com/mmogr/gglib/commit/253a52319e2071b66317b124e742c3ffb6db3a29))


### What it stopped getting wrong

* **ci:** rustdoc renders each crate after the crates it links to ([#1665](https://github.com/mmogr/gglib/issues/1665)) ([c964ee6](https://github.com/mmogr/gglib/commit/c964ee65308b42b93e3c19058acb734959c25b02)), closes [#1664](https://github.com/mmogr/gglib/issues/1664)
* **download:** a cancel is answered once the .part holds what arrived ([#1662](https://github.com/mmogr/gglib/issues/1662)) ([1932f30](https://github.com/mmogr/gglib/commit/1932f30a796dc4072ec9dc55e0284152033998d6)), closes [#1661](https://github.com/mmogr/gglib/issues/1661)
* **download:** the requirements test bounds the packages rather than pinning them ([#1671](https://github.com/mmogr/gglib/issues/1671)) ([d78338e](https://github.com/mmogr/gglib/commit/d78338e4273479775a5ec885a34c836cef4264ac)), closes [#1670](https://github.com/mmogr/gglib/issues/1670)


### Internal

* **cost:** the macOS and Windows jobs run only when a change can reach them ([#1663](https://github.com/mmogr/gglib/issues/1663)) ([cbea86a](https://github.com/mmogr/gglib/commit/cbea86a7ec87c14e96294e1c59f780824eaad06d))
* **deps:** update hf-xet requirement in /crates/gglib-download/scripts ([#1666](https://github.com/mmogr/gglib/issues/1666)) ([d88e717](https://github.com/mmogr/gglib/commit/d88e717f6dd43c071f31474a33962e62a02ca77a))
* **deps:** update huggingface-hub requirement ([#1668](https://github.com/mmogr/gglib/issues/1668)) ([67c9adb](https://github.com/mmogr/gglib/commit/67c9adb13772f14ab081fdcc89942448cb840c93))
* **hygiene:** actionlint in the gate, pinned actions, and least access per job ([#1672](https://github.com/mmogr/gglib/issues/1672)) ([c0ebf03](https://github.com/mmogr/gglib/commit/c0ebf0399fcaf9c6231814330c2eaf0257e04617))
* **release:** the release builds outlive the CI run that tags ([#1667](https://github.com/mmogr/gglib/issues/1667)) ([b8d3937](https://github.com/mmogr/gglib/commit/b8d3937693a7b659519f695ceb356e904c11414e))

## [0.29.0-rc.3](https://github.com/mmogr/gglib/compare/v0.29.0-rc.2...v0.29.0-rc.3) (2026-10-31)


### Internal

* **badges:** one crate list drives the badges, coverage and boundaries, and the badges branch is pruned ([#1652](https://github.com/mmogr/gglib/issues/1652)) ([e380b3c](https://github.com/mmogr/gglib/commit/e380b3cce6ab12ff60fd85261024117361d4a996))
* **labels:** a fix closes a triaged issue ([#1655](https://github.com/mmogr/gglib/issues/1655)) ([1820ec3](https://github.com/mmogr/gglib/commit/1820ec385cfec1b28a0b3806d6aaa81271197741))
* **release:** release-please keeps the release PR, and Promote ships the stable ([#1654](https://github.com/mmogr/gglib/issues/1654)) ([d12ed14](https://github.com/mmogr/gglib/commit/d12ed143a15b199f855716271bef3b6193123300))
