# tools

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-tools-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-tools-complexity.json)

<!-- module-docs:start -->

Central tool registry for LLM function calling. Manages registration and execution of both built-in backend tools and dynamically-loaded MCP server tools, handles name sanitization and collision detection, and stores optional React renderers for displaying tool results in the chat UI.

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
builtinIntegration  mcpIntegration  LLM caller
(fetch at startup)  (dynamic, per   (execute by name)
                     MCP server)         ▼
                                    renderers/
                                   (render result)
```

## Key Files

| File | Role |
|------|------|
| `registry.ts` | Core `ToolRegistry`; register, execute, enable/disable, name resolution |
| `types.ts` | `ToolDefinition`, `ToolExecutor`, `ToolResult`, `ToolResultRenderer`, `ParsedToolCall` |
| `builtinIntegration.ts` | Fetches built-in tool definitions from backend at startup; registers executors |
| `mcpIntegration.ts` | Registers MCP server tools dynamically; converts MCP → OpenAI format |
| `nameUtils.ts` | Name sanitization, collision detection, display name formatting |
| `renderers/` | React renderers for tool result display in the chat |

<!-- module-docs:end -->
