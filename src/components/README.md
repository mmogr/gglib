# Frontend Components

<!-- module-docs:start -->

This directory contains the React components used in the Desktop GUI (Tauri) and Web UI.

## Architecture

```text
┌─────────────┐      ┌──────────────────────────────────────┐
│   App.tsx   │ ───► │  app/Shell: sidebar/ │ the place     │
│ (Providers) │      │  Library · Endpoint  │ (router       │
└─────────────┘      │  Devices · Chats     │  outlet)      │
                     │  Settings · Diag.    │               │
                     └──────────────────────┴───────┬───────┘
                                                    │
┌───────────────────────────────────────────────────┴─┐
│ places/library/LibraryPlace                         │
│ (/library, /models/:id, /far/:id)                   │
│  ┌─────────────┬─────────────┐                      │
│  │   Library   │  inspector/ │                      │
│  │    rows     │             │                      │
│  └─────────────┴─────────────┘                      │
└─────────────────┬───────────────────────────────────┘
                  │  other addresses, lazy-loaded
       ┌──────────┴──────────────────┐
       ▼ (/chat/:id)                 ▼ (/diagnostics/:tab)
┌─────────────────────────┐   ┌──────────────────┐
│ ChatPlace (places/chat) │   │ DiagnosticsPlace │
│ ┌─────────────────────┐ │   └──────────────────┘
│ │  ChatMessagesPanel  │ │
│ └─────────────────────┘ │
└─────────────────────────┘
```

## Layout Structure

The window is a sidebar (`src/sidebar/`) beside the place the address names
(`app/Shell.tsx`); there is no header. The Library place (`src/places/library/`)
is the library's rows, with the inspector beside them while it has a subject:

1. **Library rows** (left): every model, one search, add new models
2. **Inspector** (right, `src/inspector/`): one subject the address names: a model here (how it runs, Load, Chat, Details), its log, the paired machine's model, or a Hugging Face repo

A chat is its own address: the conversation list (left) and the messages panel (right). A model's log is its inspector's (`/models/:id/log`), not a view of the chat.

## Core Components

### Layout & Navigation
- **`CopyValue.tsx`**: The ⧉ that puts one value on the clipboard and says "Copied" beside itself for one second (`COPIED_MS`), the one way a copy says it worked, never a toast; `useCopied` and `Copied` say the same word beside a copy that is not a ⧉. "Copied" is a GUI word with no Rust twin

### Model Management
- **`SamplingForm/`**: The sampling form: Settings' global defaults, the Load with options sheet's per-launch values, and (compact) the inspector's Sampling and Reasoning rows

### Chat Interface
- **`ChatMessagesPanel/`**: Message display with markdown rendering and composer
  - **`ThinkingBlock.tsx`**: A reply's fold, `▸ Thought for 3 s · 2 tool calls`, live while it thinks, opening in place onto the reasoning and one row per tool call (`ToolCallRow`: arguments, result, Copy)
  - The figure line and the action line under each turn (`TurnFoot`, `TurnActions`), always on the newest reply and on hover or focus elsewhere
  - The title ▾ (Rename, Generate title, System prompt…, Export, Start over, Delete…) and the "Custom system prompt · Edit" line
  - **Message Editing**: Inline edit of a question (Send, asked again) or a reply (Save, kept as written); an edit that would rewrite a saved reply is made on a new branch of the chat, Branch from here in every turn's ⋯, `‹ 2/3 ›` in the foot of a turn where a chat's branches part, and Retry answering a chat that ends in a question (ADR 0017)
  - **Message Deletion**: Delete… in a question's ⋯, through the one confirm sheet, with cascade deletion of subsequent messages
  - The composer: one `[+]` menu (Add image…, Use a folder…, Draw a picture, Tools ▸), chips only when set, a labelled Thinking toggle, the context ring, the model picker ▾, and Send / Stop
- **`AgentLimitsSection.tsx`**: The device-wide agent limits and reasoning controls (`services/agentOverrides`), opened from the composer's Tools ▸ › Agent limits… in a sheet. No turn carries them since a turn became its conversation and its message: D6 (`feat(gui)/chat-settings-saved`) moves them to saved settings
- **`ContextUsageDonut.tsx`**: A slot's context as a donut with its figure, for `proxy/SlotCard` (the tray and the live dashboard) until those go (T3, G1); the composer's ring is its own
- **`ToolUI/`**: A tool call's result, by its tool's renderer or the generic one
- **`ToolExecutionProgress/`**: A reply's tool calls as its fold lists them, and a running tool's progress

### Server Management
- **`ConnectClient.tsx`**: Connect a client on the endpoint place and setup's Connect step: Base URL, the API key line (a required key revealed only on Show, `KeyReveal`, which the address line shares), the Model ID with a picker, and ▸ Recipes, the daemon's per-client lines from `GET /api/proxy/clients` said as the CLI says them (`readings/connect`), each copyable

### Support Components
- **`Toast/`**: Reusable toast notification system for success/error/info messages
- **`InstallProgress.tsx`**: One engine install's latest event: a bar for the download phase, a labelled spinner for every other, the download named for its `product`; setup's Engines step, the library's llama.cpp banner, the inspector's Install llama.cpp, the image parts and the Settings engine card draw their installs with it

<!-- module-docs:end -->
