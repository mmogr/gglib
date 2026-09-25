# domain

<!-- module-docs:start -->

Core domain types.

These types represent the pure domain model, independent of any
infrastructure concerns (database, filesystem, etc.).

# Structure

- `agent` - Agent loop types (`AgentConfig`, `AgentMessage`, `AgentEvent`, etc.)
- `model` - Model types (`Model`, `NewModel`)
- `model_naming` - Shared model-naming policy (`resolve_model_name`, `NameSource`)
- `mcp` - MCP server types (`McpServer`, `NewMcpServer`, etc.)
- `chat` - Chat conversation and message types
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
