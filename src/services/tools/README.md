# tools

<!-- module-docs:start -->

Central tool registry for LLM function calling. Manages registration of both built-in backend tools and dynamically-loaded MCP server tools, handles name sanitization and collision detection, names each tool as the daemon knows it (`getBackendName`), and stores optional React renderers for displaying tool results in the chat UI. It runs nothing and chooses nothing: the composer's Tools ▸ lists it, and which tools a chat's runs may call is saved with the chat by those names (`hooks/useChatTools`); the daemon executes them.

## Architecture

```
                Tool Registry (Singleton)
      ┌────────────────────────────────────────┐
      │  tools: Map<name, RegisteredTool>       │
      │  _nameMap: Map<sanitized, originalInfo> │
      └─────────────┬──────────────────────────┘
                    │
      ┌─────────────┼──────────────────┐
      ▼             ▼                  ▼
builtinIntegration  mcpIntegration  Tools ▸ (every tool, by
(fetch at startup)  (dynamic, per    backend name)
                     MCP server)    renderers/ (render result)
```

## Key Files

| File | Role |
|------|------|
| `registry.ts` | Core `ToolRegistry`; register, name resolution, renderer lookup |
| `types.ts` | `ToolDefinition`, `RegisteredTool`, `ToolResultRenderer` |
| `builtinIntegration.ts` | Fetches built-in tool definitions from backend at startup; registers them with their renderers |
| `mcpIntegration.ts` | Registers MCP server tools dynamically; converts MCP → OpenAI format |
| `nameUtils.ts` | Name sanitization, collision detection, display name formatting |
| `renderers/` | React renderers for tool result display in the chat |

<!-- module-docs:end -->
