<!-- module-docs:start -->

# Contexts Module

React Context providers for shared application state across the gglib GUI.

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                    App.tsx                                          │
│   ┌─────────────────────────────────────────────────────────────────────────────┐   │
│   │                            Context Providers                                │   │
│   │     ┌─────────────────┐     ┌─────────────────┐                            │   │
│   │     │ SettingsContext │     │  ToastContext   │                            │   │
│   │     │   App settings  │     │  Notifications  │                            │   │
│   │     └────────┬────────┘     └────────┬────────┘                            │   │
│   │              │                       │                                      │   │
│   │              └───────────┬───────────┘                                      │   │
│   │                          ▼                                                  │   │
│   │              ┌───────────────────────┐                                      │   │
│   │              │   Child Components    │                                      │   │
│   │              │   useSettings()       │                                      │   │
│   │              │   useToast()          │                                      │   │
│   │              └───────────────────────┘                                      │   │
│   └─────────────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

## Contexts

| Context | Description |
|---------|-------------|
| [`SettingsContext.tsx`](SettingsContext.tsx) | Application settings (paths, ports, preferences) with persistence |
| [`ToastContext.tsx`](ToastContext.tsx) | Toast notification system for user feedback |

## Usage

```tsx
import { useSettings } from './contexts/SettingsContext';
import { useToast } from './contexts/ToastContext';

function MyComponent() {
  const { settings, updateSettings } = useSettings();
  const { showToast } = useToast();

  const handleSave = async () => {
    await updateSettings({ maxDownloadQueueSize: 10 });
    showToast('Settings saved!', 'success');
  };
}
```

## Design Principles

1. **Minimal Global State** — Only truly app-wide state lives in context
2. **Server as Source of Truth** — Contexts sync with backend, not replace it
3. **Type Safety** — All context values are fully typed

<!-- module-docs:end -->
