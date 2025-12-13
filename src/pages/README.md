<!-- module-docs:start -->

# Pages Module

Top-level page components for the gglib GUI application.

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                     App.tsx                                         │
│                                        │                                            │
│                     ┌──────────────────┼──────────────────┐                         │
│                     ▼                  ▼                  ▼                         │
│   ┌─────────────────────┐  ┌─────────────────────┐  ┌─────────────────────┐         │
│   │  ModelControlCenter │  │      ChatPage       │  │    (future pages)   │         │
│   │       Page          │  │                     │  │                     │         │
│   └──────────┬──────────┘  └──────────┬──────────┘  └─────────────────────┘         │
│              │                        │                                             │
│              ▼                        ▼                                             │
│   ┌─────────────────────────────────────────────────────────────────────────────┐   │
│   │                              components/                                    │   │
│   │                        (Shared UI Components)                               │   │
│   └─────────────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

## Pages

| Page | Description |
|------|-------------|
| [`ModelControlCenterPage.tsx`](ModelControlCenterPage.tsx) | Main dashboard for model management, server control, and downloads |
| [`ChatPage.tsx`](ChatPage.tsx) | Chat interface for interacting with running models |

### Model Control Center

The primary page containing:
- Model list with metadata display
- Server start/stop controls
- Download queue management
- MCP server configuration
- Settings management

### Chat Page

Interactive chat interface featuring:
- Real-time streaming responses
- Conversation history
- Model selection
- MCP tool integration

## Sub-modules

| Directory | Description |
|-----------|-------------|
| [`modelControlCenter/`](modelControlCenter/) | Components specific to the Model Control Center page |

## Styling

Each page has a corresponding CSS file:
- `ModelControlCenterPage.css` — Control center layout and styling
- `ChatPage.css` — Chat interface styling

## Design Principles

1. **Page as Composition Root** — Pages compose hooks and components
2. **Minimal Logic** — Business logic lives in hooks, not pages
3. **Responsive Layout** — Pages adapt to different screen sizes

<!-- module-docs:end -->
