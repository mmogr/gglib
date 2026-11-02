# domain

<!-- module-docs:start -->

Core domain types.

These types represent the pure domain model, independent of any
infrastructure concerns (database, filesystem, etc.).

# Structure

- `agent` - Agent loop types (`AgentConfig`, `AgentMessage`, `AgentEvent`, etc.)
- `benchmark` - Benchmark run types for the four modes (compare, perf, tune,
  agentic), their configs, results and streamed events
- `defects` - Per-model defect counters the proxy writes as events happen
  (`ModelDefectLedger`); `defects_agent` holds the one the agent loop writes,
  and `defect_counts` the counts a reader is handed (`ModelDefectCounts`,
  `LoopGuardTrip`)
- `loop_guard_log` - The loop guard's log, which survives a restart: one row
  per decision (`LoopGuardTripEvent`) and one per day, model, version and mode
  counting the requests it scanned (`LoopGuardTripDay`)
- `model` - Model types (`Model`, `NewModel`). A model reads images exactly when
  it has a projector (`Model::image_input`), and draws them exactly when its
  tensors named an image family (`Model::generates_images`)
- `model_file` - The files a model is made of, as the library records them
  (`ModelFile`, `NewModelFile`), and the folder a row is read in (`ModelFolder`)
- `model_detail` - Every stored field of one model, as the inspector reads it
  (`ModelDetailDto`), and one model as a paired machine reads it (`ModelLookup`)
- `machine` - Which machine a model is on (`Machine`), a model named by its
  machine (`ModelRef`), what may be done to a model there (`ModelAction`,
  ADR 0013's use-don't-change line as one table), and the name a machine is
  shown by, sanitised from its host name (`machine_name`)
- `model_naming` - Shared model-naming policy (`resolve_model_name`, `NameSource`)
- `query` - Model list filtering and sorting (`ModelListQuery`, `apply_query`),
  the one rule the CLI and the HTTP handler share
- `mcp` - MCP server types (`McpServer`, `NewMcpServer`, etc.)
- `chat` - Chat conversation and message types. A message carries its images
  by reference: `NewMessage.images` are ids, `Message.images` are facts
- `branching` - When a change to a saved chat branches it into a new chat
  (`plan`), and the options its family holds at each turn (`points`): a
  saved reply is never discarded or altered (ADR 0017)
- `attachment` - An image a message carries, a user's or a tool's: its id,
  the SHA-256 of its bytes (`AttachmentId`), what a client is told of it
  without the bytes (`AttachmentInfo`), and the answer to an upload
  (`AttachmentUpload`)
- `hub_chats` - The hub's chats as a paired device reads them (`HubChat`, `HubChatOpen`),
  and the turn it adds (`HubTurn`)
- `runs` - Wire shapes of a run, a reply the daemon owns (`RunInfo`, `RunStatus`)
- `gguf` - GGUF metadata and capability types
- `tensor_table` - The names and outermost-first shapes of the tensors a GGUF or
  safetensors file declares (`TensorTable`), which is all an image model's GGUF
  says about itself
- `image_family` - An image model's family (`ImageFamily`: Flux.1, SDXL,
  Qwen-Image 2.1, Z-Image), sniffed from its tensor names by stable-diffusion.cpp's
  rules; the files a family draws with beside its main weights
  (`ComponentRole`) and whether a file fits one; each family's `Recipe`, the
  one table of its components' default sources, placement, sizes and memory;
  and each variant's `Sampling` (steps, guidance, sampler, sigma preset,
  linear scale), picked from the file name where two variants share tensors.
  Its tests read the measured files' tables in `testdata/image_families/`
- `engine_pins` - The llama.cpp release gglib installs (`PINNED_LLAMA_RELEASE`),
  in this crate so that code which builds for iOS reads the pin the installer does
- `runtime_kind` - Which program serves a model (`RuntimeKind`: llama.cpp's
  `llama-server` or stable-diffusion.cpp's `sd-server`), derived from its image
  family and never stored
- `capabilities` - Model capability detection and inference
- `capability_tags` - The capability tags stored on a model and the
  predicates that read them (`capabilities_of`, `is_reasoning`, `is_embedding`)
- `dialect` - How a model wraps tool calls, as data (`DialectSpec`), which
  drives both the delimited parser and the decode-time grammar
- `template_caps` - llama-server's report of what a loaded chat template
  supports (`TemplateCaps`), held as read, unreadable or not yet read
  (`TemplateCapsState`)
- `runtime_capabilities` - What the engine under gglib can do
  (`RuntimeCapabilities`), the mirror of the GGUF capabilities
- `thinking` - A chat's Thinking choice (`Thinking`: `off` or `default`), which a
  turn says and `ConversationSettings.thinking` remembers, and the one rule
  (`thinking::settle`) that reads a turn by it at the daemon's doors and in the CLI
- `folder` - A chat's folder, where gglib's own file tools read in its runs:
  `folder::launched` for `gglib chat`'s launch directory, never home or the
  root, and `folder::chosen` for the page's pick (absolute, canonical, a
  folder, else `FolderError`); `ConversationSettings.folder` remembers the pick
- `inference` - Sampling configuration (`InferenceConfig`), the per-model
  rungs of the sampling ladder (`ModelRungs`) and where a model's defaults came
  from (`DefaultsOrigin`)
- `inference_profile` - Named, sparse sampling profiles a client selects with a
  `:{name}` suffix on the model (`InferenceProfile`), and the builtin templates
- `model_sampling` - The sampler defaults a model carries in its GGUF metadata
  (`ModelSamplingDefaults`)
- `generation_config` - The sampling recipe a model author publishes in
  `generation_config.json` (`PublishedGenerationConfig`)
- `reasoning_effort` - The `reasoning_effort` level a request asks for
  (`ReasoningEffort`)
- `sampling_provenance` - Which rung supplied each resolved sampling value
  (`FieldSources`, `ParamSource`)
- `sampling_discards` - Which values a caller named and the ladder then passed
  over (`discarded_from_rung`)
- `server_config` - Per-model server launch defaults (`ServerConfig`)
- `kv_memory` - Shape of a model's KV memory from GGUF metadata: whether it
  keeps only part of the token history (`kv_memory_is_partial`), and how many
  layers hold a per-token cache at all (`kv_cache_layer_count`)
- `kv_estimate` - Per-token KV cache size, which takes that layer count from
  `kv_memory` rather than counting every block. The dependency runs one way
  only: `kv_memory` reads metadata and knows nothing of the estimate
- `device_budget` - A device's memory budget for a model (`budget_from`), from
  one reading of its memory (`DeviceReading`), read once per serve and held
- `phone_fit` - Whether a model fits a phone, as a verdict (`FitVerdict`:
  fits, tight, too big, or unknown with the missing input named), read off the
  launch's own context fit at the default KV cache types, with the inputs it
  was reached from as the reason
- `context_fit` - How large a context this machine can serve (`fit_context`),
  the launch-time half of `gglib up`'s fit arithmetic
- `recommendation` - Picking a first model that fits the machine
  (`recommend`, `Recommendation`)
- `residency` - Whether a second model may stay resident in VRAM beside the
  first (`decide_secondary_slot`, `SecondarySlotDecision`)
- `admission` - What the admission queue and the VRAM resident set look like
  right now (`AdmissionSnapshot`), the shape `gglib-runtime` reports them in
- `cache_budget` - Auto-sizing of llama-server's host-RAM prompt cache
  (`--cache-ram`)
- `slot_eviction` - Which on-disk KV slot files to delete to stay within a byte
  budget (`select_evictions`); the scanning and deleting live in `gglib-proxy`
- `launch_narration` - What the runtime decided for one llama-server launch,
  and why (`LaunchNarration`, `LaunchDecision`)

<!-- module-docs:end -->
