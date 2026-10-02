# domain

<!-- module-docs:start -->

Core domain types.

These types represent the pure domain model, independent of any
infrastructure concerns (database, filesystem, etc.).

# Structure

- `agent` - Agent loop types (`AgentConfig`, `AgentMessage`, `AgentEvent`, etc.)
- `model` - Model types (`Model`, `NewModel`)
- `model_detail` - Every stored field of one model, as the inspector reads it
  (`ModelDetailDto`), and one model as a paired machine reads it (`ModelLookup`)
- `machine` - The name a machine is shown by, sanitised from its host name (`machine_name`)
- `model_naming` - Shared model-naming policy (`resolve_model_name`, `NameSource`)
- `mcp` - MCP server types (`McpServer`, `NewMcpServer`, etc.)
- `chat` - Chat conversation and message types
- `hub_chats` - The hub's chats as a paired device reads them (`HubChat`, `HubChatOpen`)
- `runs` - Wire shapes of a run, a reply the daemon owns (`RunInfo`, `RunStatus`)
- `gguf` - GGUF metadata and capability types
- `capabilities` - Model capability detection and inference
- `thinking` - Thinking/reasoning tag parsing and streaming accumulation
- `kv_memory` - Shape of a model's KV memory from GGUF metadata: whether it
  keeps only part of the token history (`kv_memory_is_partial`), and how many
  layers hold a per-token cache at all (`kv_cache_layer_count`)
- `kv_estimate` - Per-token KV cache size, which takes that layer count from
  `kv_memory` rather than counting every block. The dependency runs one way
  only: `kv_memory` reads metadata and knows nothing of the estimate

<!-- module-docs:end -->
