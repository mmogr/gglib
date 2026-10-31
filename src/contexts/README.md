<!-- module-docs:start -->

# Contexts Module

React Context providers for shared application state across the gglib GUI.

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                    App.tsx                                          │
│   ┌─────────────────────────────────────────────────────────────────────────────┐   │
│   │                            Context Providers                                │   │
│   │     ┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐     │   │
│   │     │ SettingsContext │     │  ToastContext   │     │ ConfirmContext  │     │   │
│   │     │   App settings  │     │  Notifications  │     │ Confirm dialog  │     │   │
│   │     └────────┬────────┘     └────────┬────────┘     └────────┬────────┘     │   │
│   │              │                       │                       │              │   │
│   │              └───────────────────────┼───────────────────────┘              │   │
│   │                                      ▼                                      │   │
│   │                          ┌───────────────────────┐                          │   │
│   │                          │   Child Components    │                          │   │
│   │                          │   useSettings()       │                          │   │
│   │                          │   useToastContext()   │                          │   │
│   │                          │   useConfirmContext() │                          │   │
│   │                          └───────────────────────┘                          │   │
│   └─────────────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

## Contexts

| Context | Description |
|---------|-------------|
| [`SettingsContext.tsx`](SettingsContext.tsx) | Application settings (paths, ports, preferences) with persistence |
| [`ToastContext.tsx`](ToastContext.tsx) | Toast notification system for user feedback |
| [`ConfirmContext.tsx`](ConfirmContext.tsx) | `confirm(opts)` returning a promise that settles when the user answers the dialog |

## Usage

```tsx
import { useSettings } from './hooks/useSettings';
import { useToastContext } from './contexts/ToastContext';

function MyComponent() {
  const { settings, save } = useSettings();
  const { showToast } = useToastContext();

  const handleSave = async () => {
    await save({ maxDownloadQueueSize: 10 });
    showToast('Settings saved!', 'success');
  };
}
```

## Design Principles

1. **Minimal Global State** — Only truly app-wide state lives in context
2. **Server as Source of Truth** — Contexts sync with backend, not replace it
3. **Type Safety** — All context values are fully typed

<!-- module-docs:end -->
