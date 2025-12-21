<!-- module-docs:start -->

# Types Module

TypeScript type definitions shared across the gglib GUI.

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                  types/index.ts                                     │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌─────────────────────────────────────────────────────────────────────────────┐    │
│  │                             Domain Types                                    │    │
│  │  GgufModel, ServerInfo, DownloadConfig, ServeConfig, etc.                   │    │
│  └─────────────────────────────────────────────────────────────────────────────┘    │
│                                                                                     │
│  ┌─────────────────────────────────────────────────────────────────────────────┐    │
│  │                            Settings Types                                   │    │
│  │  AppSettings, ModelsDirectoryInfo, etc.                                     │    │
│  └─────────────────────────────────────────────────────────────────────────────┘    │
│                                                                                     │
│  ┌─────────────────────────────────────────────────────────────────────────────┐    │
│  │                             Event Types                                     │    │
│  │  ServerEvent, DownloadEvent, etc.                                           │    │
│  └─────────────────────────────────────────────────────────────────────────────┘    │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
                                       │
                                       ▼
                          ┌───────────────────────┐
                          │    Rust Backend       │
                          │  (gglib-core types)   │
                          └───────────────────────┘
```

## Key Types

### Domain Types

| Type | Description |
|------|-------------|
| `GgufModel` | Model metadata (name, path, params, quantization, tags) |
| `ServerInfo` | Running server state (model, port, status) |
| `DownloadConfig` | Download request parameters |
| `ServeConfig` | Server launch configuration |

### Configuration Types

| Type | Description |
|------|-------------|
| `AppSettings` | Application preferences and defaults |
| `ModelsDirectoryInfo` | Models directory path and metadata |

### Event Types

| Type | Description |
|------|-------------|
| `ServerEvent` | Server lifecycle events (running, stopped, crashed) |
| `DownloadProgress` | Download progress updates |

## Type Alignment

These TypeScript types mirror the Rust types in `gglib-core`:

```text
TypeScript                    Rust
──────────────────────────────────────────
GgufModel          ←→        Model
ServerInfo         ←→        RunningServer
AppSettings        ←→        Settings
DownloadProgress   ←→        DownloadProgress
```

The `services/transport/` layer handles JSON serialization between these.

## Usage

```typescript
import type { GgufModel, ServerInfo, AppSettings } from './types';

function ModelCard({ model }: { model: GgufModel }) {
  return (
    <div>
      <h3>{model.name}</h3>
      <p>{model.param_count_b}B params</p>
    </div>
  );
}
```

## Design Principles

1. **Single Source** — All shared types exported from `index.ts`
2. **Backend Parity** — Types match Rust structs for seamless JSON exchange
3. **Strict Typing** — No `any` types; full type coverage

<!-- module-docs:end -->
