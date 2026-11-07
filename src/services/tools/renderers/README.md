# renderers

<!-- module-docs:start -->

React components and rendering logic for displaying tool execution results in the chat UI. Each renderer is a plain object implementing `renderResult()`, which draws the result under a tool call's arguments in a reply's fold. Renderers are stored on `RegisteredTool` objects in the tool registry and looked up at render time.

## Key Files

| File | Role |
|------|------|
| `FallbackRenderer.tsx` | Default JSON viewer with syntax highlighting; used when no specific renderer matches |
| `TimeRenderer.tsx` | Human-readable date/time formatting for `get_current_time` tool results |
| `McpGenericRenderer.tsx` | Generic MCP renderer; auto-detects Markdown, table arrays, and structured objects |

## Dispatch Flow

```
A tool call's row in a reply's fold, opened → ToolResultDisplay(toolName, result)
       ▼
  registry.getRenderer(toolName)
       ├── Found: renderer.renderResult(result) → ReactNode
       └── Not found: FallbackRenderer.renderResult(result) → ReactNode
       ▼
  Drawn under the call's arguments, then Copy
```

<!-- module-docs:end -->
