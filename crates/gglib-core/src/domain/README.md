# domain

<!-- module-docs:start -->

Core domain types.

These types represent the pure domain model, independent of any
infrastructure concerns (database, filesystem, etc.).

# Structure

- `agent` - Agent loop types (`AgentConfig`, `AgentMessage`, `AgentEvent`, etc.)
- `model` - Model types (`Model`, `NewModel`). A model reads images exactly when
  it has a projector (`Model::image_input`), and draws them exactly when its
  tensors named an image family (`Model::generates_images`)
- `model_file` - The files a model is made of, as the library records them
  (`ModelFile`, `NewModelFile`)
- `model_detail` - Every stored field of one model, as the inspector reads it
  (`ModelDetailDto`), and one model as a paired machine reads it (`ModelLookup`)
- `machine` - Which machine a model is on (`Machine`), a model named by its
  machine (`ModelRef`), what may be done to a model there (`ModelAction`,
  ADR 0013's use-don't-change line as one table), and the name a machine is
  shown by, sanitised from its host name (`machine_name`)
- `model_naming` - Shared model-naming policy (`resolve_model_name`, `NameSource`)
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
  Qwen-Image 2.1), sniffed from its tensor names by stable-diffusion.cpp's
  rules; the files a family draws with beside its main weights
  (`ComponentRole`) and whether a file fits one; and each family's `Recipe`,
  the one table of its components' default sources and its drawing defaults.
  Its tests read the measured files' tables in `testdata/image_families/`
- `capabilities` - Model capability detection and inference
- `thinking` - A chat's Thinking choice (`Thinking`: `off` or `default`), which a
  turn says and `ConversationSettings.thinking` remembers, and the one rule
  (`thinking::settle`) that reads a turn by it at the daemon's doors and in the CLI
- `kv_memory` - Shape of a model's KV memory from GGUF metadata: whether it
  keeps only part of the token history (`kv_memory_is_partial`), and how many
  layers hold a per-token cache at all (`kv_cache_layer_count`)
- `kv_estimate` - Per-token KV cache size, which takes that layer count from
  `kv_memory` rather than counting every block. The dependency runs one way
  only: `kv_memory` reads metadata and knows nothing of the estimate

<!-- module-docs:end -->
