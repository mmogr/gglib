# gglib-voice

Voice mode pipeline for gglib — fully local speech-to-text and text-to-speech.

## Overview

This crate provides seamless voice conversation capabilities:

1. **Audio Capture** — microphone input via `cpal`, resampled to 16kHz mono
2. **STT (Speech-to-Text)** — local transcription via `whisper-rs` (whisper.cpp)
3. **TTS (Text-to-Speech)** — local synthesis via `kokoro-tts` (Kokoro v1.0, 82M params)
4. **VAD (Voice Activity Detection)** — utterance detection via Silero VAD
5. **Echo Gate** — prevents TTS output from triggering STT in speaker mode
6. **Voice Pipeline** — orchestrates the full conversation loop

## Architecture

```text
┌─────────────────────────────────────────────────┐
│                Voice Pipeline                    │
│                                                  │
│  ┌──────────┐  ┌───────┐  ┌───────┐  ┌───────┐ │
│  │ Capture  │→ │  VAD  │→ │  STT  │→ │ (LLM) │ │
│  │ (cpal)   │  │(silero│  │(whis- │  │ resp) │ │
│  └──────────┘  │  vad) │  │per.cpp│  └───┬───┘ │
│       ↑        └───────┘  └───────┘      │     │
│       │                                   ▼     │
│  ┌────┴─────┐                        ┌───────┐ │
│  │Echo Gate │←───────────────────────│  TTS  │ │
│  │(AtomicBool)                       │(kokoro│ │
│  └──────────┘                        └───┬───┘ │
│                                          │     │
│                                     ┌────▼───┐ │
│                                     │Playback│ │
│                                     │(rodio) │ │
│                                     └────────┘ │
└─────────────────────────────────────────────────┘
```

## Model Requirements

| Component | Model | Format | Size |
|-----------|-------|--------|------|
| STT | whisper.cpp (base.en default) | GGML `.bin` | 142 MB |
| TTS | Kokoro v1.0 | ONNX | ~330 MB |
| VAD | Silero VAD | ONNX (via whisper.cpp) | ~2 MB |

All models are downloaded on first voice activation and stored in
`~/.local/share/gglib/voice_models/`.

## Features

- `coreml` — Apple Silicon acceleration for Kokoro TTS via CoreML
- `cuda` — NVIDIA GPU acceleration for Kokoro TTS

## Dependencies

Zero system install dependencies. No espeak-ng required — `kokoro-tts`
includes a built-in grapheme-to-phoneme pipeline.
