# gglib-axum

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-complexity.json)

HTTP API server for gglib — provides REST endpoints for the web UI and external integrations.

## Architecture

This crate is in the **Adapter Layer** — it exposes gglib functionality via HTTP using the Axum framework.

```text
                              ┌──────────────────┐
                              │   gglib-axum     │
                              │   HTTP server    │
                              └────────┬─────────┘
                                       │
         ┌─────────────┬───────────────┼───────────────┬─────────────┬─────────────┐
         ▼             ▼               ▼               ▼             ▼             ▼
┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐
│  gglib-db   │ │gglib-download│ │gglib-runtime│ │  gglib-hf   │ │  gglib-mcp  │ │gglib-voice  │
│   SQLite    │ │  Downloads  │ │   Servers   │ │  HF client  │ │ MCP servers │ │ Audio/WS I/O│
└─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘
         │             │               │               │             │             │
         └─────────────┴───────────────┴───────────────┴─────────────┴─────────────┘
                                       │
                                       ▼
                              ┌──────────────────┐
                              │    gglib-core    │
                              │   (all ports)    │
                              └──────────────────┘
```

See the [Architecture Overview](../../README.md#architecture-overview) for the complete diagram.

## Internal Structure

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                gglib-axum                                           │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐                            │
│  │   main.rs   │ ──► │ bootstrap.rs│ ──► │  routes.rs  │                            │
│  │  Entry pt   │     │  DI setup   │     │   Router    │                            │
│  │             │     │  & wiring   │     │  mounting   │                            │
│  └─────────────┘     └─────────────┘     └─────────────┘                            │
│                                                                                     │
│  ┌───────────┐     ┌───────────┐     ┌───────────┐                            │
│  │    dto/     │     │  error.rs   │     │ ws_audio.rs │                            │
│  │  Request &  │     │  HTTP error │     │ WS audio    │                            │
│  │  Response   │     │  handling   │     │ source/sink │                            │
│  └───────────┘     └───────────┘     └───────────┘                            │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`bootstrap.rs`](src/bootstrap.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-bootstrap-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-bootstrap-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-bootstrap-coverage.json) |
| [`chat_api.rs`](src/chat_api.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-chat_api-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-chat_api-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-chat_api-coverage.json) |
| [`embedded.rs`](src/embedded.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-embedded-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-embedded-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-embedded-coverage.json) |
| [`error.rs`](src/error.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-error-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-error-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-error-coverage.json) |
| [`routes.rs`](src/routes.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-routes-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-routes-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-routes-coverage.json) |
| [`sse.rs`](src/sse.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-sse-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-sse-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-sse-coverage.json) |
| [`state.rs`](src/state.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-state-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-state-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-state-coverage.json) |
| [`ws_audio.rs`](src/ws_audio.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-ws_audio-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-ws_audio-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-ws_audio-coverage.json) |
| [`dto/`](src/dto/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-dto-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-dto-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-dto-coverage.json) |
| [`handlers/`](src/handlers/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-coverage.json) |
<!-- module-table:end -->

</details>

**Module Descriptions:**
- **`bootstrap.rs`** — Dependency injection and service wiring
- **`chat_api.rs`** — Chat completion API endpoints and streaming
- **`error.rs`** — HTTP error types and JSON error responses
- **`routes.rs`** — Route definitions and handler mounting
- **`sse.rs`** — Server-Sent Events utilities for streaming
- **`ws_audio.rs`** — `WebSocketAudioSource` and `WebSocketAudioSink`: mpsc-backed `AudioSource`/`AudioSink` implementations that bridge browser PCM16 LE audio over a WebSocket binary channel
- **`dto/`** — Request/response DTOs for API endpoints
- **`handlers/verification.rs`** — Model verification, update checking, and repair endpoints
- **`handlers/voice.rs`** — 19 thin Axum handlers for voice data/config operations and audio control endpoints
- **`handlers/voice_ws.rs`** — WebSocket upgrade handler (`GET /api/voice/audio`): registers `WebSocketAudioSource`/`WebSocketAudioSink` with `VoiceService`, spawns ingest/egress tasks

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/models` | List all models |
| `POST` | `/api/models` | Add a new model |
| `DELETE` | `/api/models/:id` | Remove a model |
| `POST` | `/api/serve/:id` | Start llama-server |
| `DELETE` | `/api/serve/:id` | Stop llama-server |
| `POST` | `/api/hf/search` | Search HuggingFace |
| `POST` | `/api/download` | Queue a download |
| `GET` | `/api/download/:id` | Get download status |
| `GET` | `/api/mcp/servers` | List MCP servers |
| `POST` | `/api/mcp/servers/:id/start` | Start MCP server |
| `POST` | `/api/models/:id/verify` | Verify model integrity (streams progress via SSE) |
| `GET` | `/api/models/:id/updates` | Check for HuggingFace updates |
| `POST` | `/api/models/:id/repair` | Re-download corrupt shards |
| `GET` | `/api/voice/status` | Voice pipeline state and loaded models |
| `GET` | `/api/voice/models` | Voice model catalog with download status |
| `GET` | `/api/voice/devices` | OS audio input devices |
| `POST` | `/api/voice/models/stt/{id}/download` | Download STT model |
| `POST` | `/api/voice/models/tts/download` | Download TTS bundle |
| `POST` | `/api/voice/models/vad/download` | Download Silero VAD |
| `POST` | `/api/voice/stt/load` | Load STT model into pipeline |
| `POST` | `/api/voice/tts/load` | Load TTS model into pipeline |
| `PUT` | `/api/voice/mode` | Set PTT / VAD interaction mode |
| `PUT` | `/api/voice/voice` | Set active TTS voice |
| `PUT` | `/api/voice/speed` | Set TTS playback speed |
| `PUT` | `/api/voice/auto-speak` | Enable/disable auto-TTS on LLM responses |
| `POST` | `/api/voice/unload` | Stop audio I/O and release model memory |
| `POST` | `/api/voice/start` | Start voice pipeline (PTT or VAD mode) |
| `POST` | `/api/voice/stop` | Stop voice pipeline |
| `POST` | `/api/voice/ptt-start` | Begin push-to-talk recording |
| `POST` | `/api/voice/ptt-stop` | Stop PTT recording and transcribe |
| `POST` | `/api/voice/speak` | Synthesize and play TTS (202 fire-and-forget) |
| `POST` | `/api/voice/stop-speaking` | Interrupt TTS playback |
| `GET` | `/api/voice/audio` | WebSocket upgrade — binary PCM16 LE audio data plane |

## Usage

```bash
# Start the API server
gglib-axum --port 9887

# Or run from the workspace
cargo run --package gglib-axum -- --port 9887
```

```rust,ignore
// Programmatic usage
use gglib_axum::start_server;
use gglib_axum::bootstrap::ServerConfig;

async fn run() -> anyhow::Result<()> {
    let config = ServerConfig::with_defaults()?;
    start_server(config).await
}
```

## Design Decisions

1. **Axum Framework** — Chosen for async-first design and tower middleware ecosystem
2. **Shared GuiBackend** — Same façade as Tauri for feature parity
3. **Thin Handlers** — No logic, just parse → delegate → serialize
4. **CORS Support** — Configurable CORS for web UI development
