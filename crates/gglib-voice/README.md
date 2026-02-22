# gglib-voice

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-complexity.json)

Voice mode pipeline for gglib — fully local speech-to-text, text-to-speech, and voice activity detection, exposed to the rest of the platform via `VoicePipelinePort`.

## Architecture

This crate is in the **Infrastructure Layer** — it manages audio I/O, speech recognition, and speech synthesis using native OS audio APIs and on-device ML models.

See the [Architecture Overview](../../README.md#architecture-overview) for the complete diagram.

## Overview

This crate provides seamless voice conversation capabilities:

1. **Audio Capture** — microphone input via `cpal`, resampled to 16 kHz mono
2. **STT (Speech-to-Text)** — local transcription via sherpa-onnx (Whisper ONNX models)
3. **TTS (Text-to-Speech)** — local synthesis via sherpa-onnx (Kokoro v0.19 English, 11 voices)
4. **VAD (Voice Activity Detection)** — Silero neural-net VAD via sherpa-onnx, with energy-based fallback
5. **Echo Gate** — prevents TTS output from triggering STT in speaker mode
6. **Voice Pipeline** — orchestrates the full conversation loop as a state machine
7. **Audio Thread** — actor pattern isolating `!Send` audio types on a dedicated OS thread
8. **Audio I/O Abstraction** — `AudioSource`/`AudioSink` traits for pluggable backends (local cpal/rodio or WebSocket)
9. **VoiceService** — implements `VoicePipelinePort` and `RemoteAudioRegistry`; bridges `VoiceEvent` → `AppEvent` with AudioLevel throttling

## Internal Structure

```text
┌─────────────────────────────────────────────────────────────┐
│                 gglib-voice (this crate)                    │
├─────────────────────────────────────────────────────────────┤
│  VoiceService (VoicePipelinePort + RemoteAudioRegistry)     │
│    └── Bridges VoiceEvent → AppEvent (AudioLevel throttled) │
│    └── Selects LocalAudio* or WebSocket* on start()         │
│                                                             │
│  VoicePipeline (state machine)                              │
│    Idle→Listening→Recording→Transcribing→Thinking→Speaking  │
│    Uses injected AudioSource / AudioSink trait objects      │
│                                                             │
│  AudioSource / AudioSink traits (audio I/O abstraction)     │
│    ├── LocalAudioSource/LocalAudioSink: cpal/rodio via      │
│    │     AudioThreadHandle (same Arc, dedicated OS thread)  │
│    └── WebSocketAudioSource/WebSocketAudioSink: in          │
│          gglib-axum (mpsc channel bridge to browser WS)     │
│                                                             │
│  AudioThreadHandle (actor)                                  │
│    └── Isolates !Send cpal/rodio on dedicated OS thread     │
│    └── All comms via std::sync::mpsc channels               │
│                                                             │
│  backend/                                                   │
│    ├── SttBackend / TtsBackend traits (engine-agnostic)     │
│    ├── sherpa_stt: Whisper ONNX via sherpa-rs               │
│    └── sherpa_tts: Kokoro v0.19 via sherpa-rs               │
│                                                             │
│  capture: Microphone capture (cpal) + resampling (rubato)   │
│  playback: Audio playback (rodio) + streaming               │
│  vad: Silero neural-net VAD + energy-based fallback         │
│  gate: Echo gate (AtomicBool suppression during TTS)        │
│  models: Model catalog, download, and path management       │
│  text_utils: Markdown stripping + sentence chunking for TTS │
│  error: Unified VoiceError enum                             │
└─────────────────────────────────────────────────────────────┘
                           │
           depends on
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                      gglib-core                             │
│  domain/settings.rs  │  paths.rs (data directory)           │
└─────────────────────────────────────────────────────────────┘
```

## Key Design Decisions

### Safe Audio Threading

`cpal::Stream` and `rodio::OutputStream` are `!Send` on some platforms (macOS CoreAudio). Rather than using `unsafe impl Send/Sync`, the crate confines both types to a dedicated OS thread via `AudioThreadHandle` — an actor that communicates through `std::sync::mpsc` channels. The crate maintains `unsafe_code = "deny"`.

### Audio I/O Abstraction

`AudioSource` and `AudioSink` are object-safe traits that decouple the pipeline from any specific audio backend. `LocalAudioSource`/`LocalAudioSink` wrap the existing `AudioThreadHandle` (cpal/rodio) and share one `Arc` (no `Mutex` needed since every method takes `&self`). `WebSocketAudioSource`/`WebSocketAudioSink` live in `gglib-axum` and bridge browser PCM over a WebSocket binary channel. The pipeline calls `start_with_audio(source, sink)` and never knows which backend is active.

### Service Layer

`VoiceService` implements `VoicePipelinePort` (for `gglib-gui`/`gglib-axum`) and `RemoteAudioRegistry` (for the WebSocket handler). It owns the `Arc<RwLock<Option<VoicePipeline>>>`, an `AppEventEmitter` for event forwarding, and a `pending_remote` slot for registering a WebSocket audio session before `start()` is called. On `start()`, it checks `pending_remote` and injects the appropriate backends.

### Backend Abstraction

STT and TTS are accessed through engine-agnostic traits (`SttBackend`, `TtsBackend`). The current implementation uses sherpa-onnx for both, but the abstraction allows swapping engines without touching the pipeline logic.

### Model Lifecycle

Models are downloaded lazily on first use from [sherpa-onnx releases](https://github.com/k2-fsa/sherpa-onnx/releases) and cached at `~/.local/share/gglib/voice_models/`. Pre-built sherpa-onnx static libraries are downloaded at build time — no system install of espeak-ng or other native libraries is required.

## Model Requirements

| Component | Model | Format | Size |
|-----------|-------|--------|------|
| STT | Whisper (7 sizes, tiny → large-v3-turbo) | ONNX (tar.bz2 archive) | 111 MB – 610 MB |
| TTS | Kokoro v0.19 English (11 voices) | ONNX (tar.bz2 archive) | ~305 MB |
| VAD | Silero VAD v5 | ONNX (single file) | ~629 KB |

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`audio_io.rs`](src/audio_io.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-audio_io-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-audio_io-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-audio_io-coverage.json) |
| [`audio_local.rs`](src/audio_local.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-audio_local-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-audio_local-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-audio_local-coverage.json) |
| [`audio_thread.rs`](src/audio_thread.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-audio_thread-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-audio_thread-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-audio_thread-coverage.json) |
| [`capture.rs`](src/capture.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-capture-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-capture-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-capture-coverage.json) |
| [`error.rs`](src/error.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-error-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-error-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-error-coverage.json) |
| [`gate.rs`](src/gate.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-gate-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-gate-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-gate-coverage.json) |
| [`models.rs`](src/models.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-models-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-models-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-models-coverage.json) |
| [`pipeline.rs`](src/pipeline.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-pipeline-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-pipeline-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-pipeline-coverage.json) |
| [`service.rs`](src/service.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-service-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-service-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-service-coverage.json) |
| [`playback.rs`](src/playback.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-playback-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-playback-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-playback-coverage.json) |
| [`stt.rs`](src/stt.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-stt-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-stt-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-stt-coverage.json) |
| [`text_utils.rs`](src/text_utils.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-text_utils-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-text_utils-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-text_utils-coverage.json) |
| [`tts.rs`](src/tts.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-tts-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-tts-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-tts-coverage.json) |
| [`vad.rs`](src/vad.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-vad-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-vad-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-vad-coverage.json) |
| [`backend/`](src/backend/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-backend-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-backend-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-backend-coverage.json) |
<!-- module-table:end -->

</details>

**Module Descriptions:**
- **`audio_io.rs`** — `AudioSource` and `AudioSink` object-safe traits; the seam between the pipeline and any audio backend
- **`audio_local.rs`** — `LocalAudioSource`/`LocalAudioSink`: thin adapters over `Arc<AudioThreadHandle>` (cpal/rodio), sharing one OS thread
- **`audio_thread.rs`** — Dedicated OS thread actor for `!Send` audio I/O (cpal/rodio), communicates via `mpsc` channels
- **`capture.rs`** — Microphone capture via `cpal`, resampling to 16 kHz mono via `rubato`
- **`error.rs`** — `VoiceError` enum covering capture, playback, STT, TTS, VAD, and model errors
- **`gate.rs`** — Echo gate using `AtomicBool` to suppress mic capture during TTS playback
- **`models.rs`** — Model catalog (7 STT + 1 TTS + 1 VAD), download orchestration, and path management
- **`pipeline.rs`** — Voice pipeline state machine (Idle→Listening→Recording→Transcribing→Speaking); accepts injected `AudioSource`/`AudioSink` via `start_with_audio()`
- **`service.rs`** — `VoiceService`: implements `VoicePipelinePort` (19 operations) and `RemoteAudioRegistry`; bridges `VoiceEvent` → `AppEvent` with 50 ms AudioLevel throttle; selects local vs WebSocket audio backend on `start()`
- **`playback.rs`** — Audio playback via `rodio` with streaming append and completion detection
- **`stt.rs`** — STT engine wrapper providing `transcribe()` over the loaded backend
- **`text_utils.rs`** — Markdown stripping, thinking-block removal, and sentence-boundary chunking for TTS
- **`tts.rs`** — TTS engine wrapper with voice selection and speed control
- **`vad.rs`** — Voice activity detection: Silero neural-net VAD with energy-based fallback
- **`backend/`** — Engine-agnostic `SttBackend`/`TtsBackend` traits + sherpa-onnx implementations
