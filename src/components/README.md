# Frontend Components

This directory contains the React components used in the Desktop GUI (Tauri) and Web UI.

## Architecture

```text
┌─────────────┐      ┌────────────────┐
│   App.tsx   │ ───► │    Header      │
│  (Router)   │      │ (Settings/Srv) │
└──────┬──────┘      └───────┬────────┘
       │                     │
       ▼                     ▼
┌─────────────────────────────────────┐
│      ModelControlCenterPage         │
│  ┌─────────────┬─────────────┐      │
│  │   Library   │  Inspector  │      │
│  │    Panel    │    Panel    │      │
│  └─────────────┴─────────────┘      │
└─────────────────────────────────────┘
              │
              ▼ (when model served)
┌─────────────────────────────────────┐
│            ChatPage                 │
│  ┌─────────────┬─────────────┐      │
│  │Conversation │   Messages  │      │
│  │    List     │    Panel    │      │
│  └─────────────┴─────────────┘      │
└─────────────────────────────────────┘
```

## Layout Structure

The UI uses a clean 2-panel layout:

1. **Model Library Panel** (left): Browse models, search/filter, add new models
2. **Model Inspector Panel** (right): View details, serve models, manage servers

When a model is served, the view transitions to a Chat layout with tab switching:
1. **Chat View**: Conversation List (left) + Messages Panel (right)
2. **Console View**: Server Info Panel (left) + Live Logs (right)

## Core Components

### Layout & Navigation
- **`Header.tsx`**: Top navigation with settings button and running servers popover
- **`RunsPopover/`**: Displays active llama-server processes with stop controls
- **`SettingsModal.tsx`**: Configuration modal for models directory and preferences

### Model Management
- **`ModelLibraryPanel/`**: Main model browser with tabbed interface:
  - "Your Models" tab: List of imported models with search/filter
  - "Add Models" tab: Add local files or download from HuggingFace
- **`ModelInspectorPanel/`**: Detailed model view with serve/stop controls
- **`ModelList.tsx`**: Compact list view of models
- **`AddModel.tsx`**: Add model from local file
- **`DownloadModel.tsx`**: Download models from HuggingFace
- **`HuggingFaceBrowser/`**: Browse and search GGUF models on HuggingFace Hub

### Chat Interface
- **`ChatMessagesPanel/`**: Message display with markdown rendering and composer
  - **`ThinkingBlock.tsx`**: Collapsible "Thinking" section for reasoning models, shows live duration during streaming and final "Thought for X seconds" on completion
  - **`ConfirmDeleteModal.tsx`**: Modal dialog for confirming message deletion with cascade warning
  - AI-generated title button (✨) for auto-naming conversations via LLM
  - **Message Editing**: Inline edit mode for user messages with Save & Regenerate
  - **Message Deletion**: Delete button with cascade deletion of subsequent messages
- **`ConversationListPanel/`**: Conversation list with search and management controls

### Console View
When a model is served, users can switch between Chat and Console views:
- **`ConsoleInfoPanel/`**: Left panel showing server info (port, uptime, context usage), live metrics from `/metrics` endpoint, and stop button. Uses `useServerState` hook to subscribe to backend lifecycle events - polling automatically stops when server stops and resumes when a new `server:running` event is received.

### Server Management
- **`ServerStatus.tsx`**: Display server health and status
- **`ServerList/`**: List of running server instances
- **`ProxyControl.tsx`**: OpenAI-compatible proxy controls
- **`LlamaInstallModal.tsx`**: llama.cpp installation wizard

### Support Components
- **`DownloadProgressDisplay/`**: Download progress indicators
- **`Toast/`**: Reusable toast notification system for success/error/info messages
