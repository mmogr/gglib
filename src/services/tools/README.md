# tools

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-tools-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-tools-complexity.json)

<!-- module-docs:start -->

Central tool registry for LLM function calling. Manages registration and enablement of both built-in backend tools and dynamically-loaded MCP server tools, handles name sanitization and collision detection, and stores optional React renderers for displaying tool results in the chat UI. It runs nothing: a run sends the enabled tools' backend names as its `tool_filter`, and the daemon executes them.

## Architecture

```
                Tool Registry (Singleton)
      ┌────────────────────────────────────────┐
      │  tools: Map<name, RegisteredTool>       │
      │  enabledTools: Set<name>                │
      │  _nameMap: Map<sanitized, originalInfo> │
      └─────────────┬──────────────────────────┘
                    │
      ┌─────────────┼──────────────────┐
      ▼             ▼                  ▼
builtinIntegration  mcpIntegration  run request (enabled tools,
(fetch at startup)  (dynamic, per    by backend name)
                     MCP server)    renderers/ (render result)
```

## Key Files

| File | Role |
|------|------|
| `registry.ts` | Core `ToolRegistry`; register, enable/disable, name resolution, renderer lookup |
| `types.ts` | `ToolDefinition`, `RegisteredTool`, `ToolResultRenderer` |
| `builtinIntegration.ts` | Fetches built-in tool definitions from backend at startup; registers them with their renderers |
| `mcpIntegration.ts` | Registers MCP server tools dynamically; converts MCP → OpenAI format |
| `nameUtils.ts` | Name sanitization, collision detection, display name formatting |
| `renderers/` | React renderers for tool result display in the chat |

<!-- module-docs:end -->
