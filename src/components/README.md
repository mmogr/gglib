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
       ▼ (/chat/:id)                 ▼ (/diagnostics/benchmarks)
┌─────────────────────────┐   ┌───────────────┐
│        ChatPage         │   │ BenchmarkPage │
│ ┌────────────┬────────┐ │   └───────────────┘
│ │Conversation│Messages│ │
│ │    List    │ Panel  │ │
│ └────────────┴────────┘ │
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
- **`SettingsModal.tsx`**: Configuration modal for models directory and preferences, opened from the sidebar's foot
- **`CopyValue.tsx`**: The ⧉ that puts one value on the clipboard and tells its caller, which says "copied" where it lives

### Model Management
- **`SamplingForm/`**: The sampling form: Settings' global defaults, the Load with options sheet's per-launch values, and (compact) the inspector's Sampling and Reasoning rows

### Chat Interface
- **`ChatMessagesPanel/`**: Message display with markdown rendering and composer
  - **`ThinkingBlock.tsx`**: Collapsible "Thinking" section for reasoning models, shows live duration during streaming and final "Thought for X seconds" on completion
  - **`ConfirmDeleteModal.tsx`**: Modal dialog for confirming message deletion with cascade warning
  - AI-generated title button (✨) for auto-naming conversations via LLM
  - **Message Editing**: Inline edit of a question (Send, asked again) or a reply (Save, kept as written); an edit that would rewrite a saved reply is made on a new branch of the chat, Branch from here on every turn, the options where a chat's branches part shown in the turn's margin, and Retry answering a chat that ends in a question (ADR 0017)
  - **Message Deletion**: Delete button with cascade deletion of subsequent messages
- **`ConversationListPanel/`**: Conversation list with search and management controls

### Server Management
- **`ConnectClient.tsx`**: Connect a client on the endpoint place: Base URL, the API key line (a required key revealed only on Show, `KeyReveal`, which the address line shares), the Model ID with a picker, and ▸ Recipes, the daemon's per-client lines from `GET /api/proxy/clients` said as the CLI says them (`readings/connect`), each copyable
- **`RemoteControl.tsx`**: `RemotePopover`, the remote tunnel panel (ADR 0012) — this machine's proxy on another machine, or another's here; its two halves live in `remote/`; anchored by the sidebar's devices line until the Devices place replaces it

### Support Components
- **`Toast/`**: Reusable toast notification system for success/error/info messages

<!-- module-docs:end -->
