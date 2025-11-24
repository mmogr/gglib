# Frontend Components

This directory contains the React components used in the Desktop GUI (Tauri) and Web UI.

## Architecture

```text
┌─────────────┐      ┌────────────────┐
│   App.tsx   │ ───► │   Layout/UI    │
│  (Router)   │      │ (Sidebar/Head) │
└──────┬──────┘      └───────┬────────┘
       │                     │
       ▼                     ▼
┌─────────────┐      ┌────────────────┐
│    Pages    │ ───► │   Components   │
│ (Views/Rts) │      │ (ModelList...) │
└─────────────┘      └────────────────┘
```

## Structure

- **`App.tsx`**: Main application entry point and router configuration.
- **`Sidebar.tsx` / `Header.tsx`**: Core layout components.
- **`SettingsModal.tsx`**: Gear-triggered modal for configuring the shared models directory.
- **`ModelList.tsx`**: The main view for listing and managing models.
- **`ChatView.tsx`**: The chat interface component.
- **`DownloadModel.tsx`**: Component for searching and downloading models.
- **`ModelInspectorPanel/`**: Detailed view for model metadata.
- **`WorkPanel/`**: Container for active tasks and views.
