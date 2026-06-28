# renderers

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-tools-renderers-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-tools-renderers-complexity.json)

<!-- module-docs:start -->

React components and rendering logic for displaying tool execution results in the chat UI. Each renderer is a plain object implementing `renderResult()` for full display and optionally `renderSummary()` for compact inline headers. Renderers are stored on `RegisteredTool` objects in the tool registry and looked up at render time.

## Key Files

| File | Role |
|------|------|
| `FallbackRenderer.tsx` | Default JSON viewer with syntax highlighting; used when no specific renderer matches |
| `TimeRenderer.tsx` | Human-readable date/time formatting for `get_current_time` tool results |
| `McpGenericRenderer.tsx` | Generic MCP renderer; auto-detects Markdown, table arrays, and structured objects |
| `McpSchemaRenderer.tsx` | Schema-driven renderer; dynamically builds UI from JSON Schema definitions |
| `SchemaBasedView.tsx` | Reusable view component shared by schema and fallback renderers |

## Dispatch Flow

```
Tool execution complete → ToolResultDisplay(toolName, result)
       ▼
  registry.getRenderer(toolName)
       ├── Found: renderer.renderResult(result) → ReactNode
       └── Not found: FallbackRenderer.renderResult(result) → ReactNode
       ▼
  Inserted into ChatMessage content
```

<!-- module-docs:end -->
