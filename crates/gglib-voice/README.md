# gglib-voice

Voice mode pipeline for gglib — fully local speech-to-text and text-to-speech.

## Overview

This crate provides seamless voice conversation capabilities:

1. **Audio Capture** — microphone input via `cpal`, resampled to 16 kHz mono
2. **STT (Speech-to-Text)** — local transcription via sherpa-onnx (Whisper ONNX models)
3. **TTS (Text-to-Speech)** — local synthesis via sherpa-onnx (Kokoro v0.19, 82M params)
4. **VAD (Voice Activity Detection)** — Silero neural-net VAD via sherpa-onnx, with energy-based fallback
5. **Echo Gate** — prevents TTS output from triggering STT in speaker mode
6. **Voice Pipeline** — orchestrates the full conversation loop

## Architecture

```text
┌─────────────────────────────────────────────────┐
│                Voice Pipeline                    │
│                                                  │
│  ┌──────────┐  ┌───────┐  ┌───────┐  ┌───────┐ │
│  │ Capture  │→ │  VAD  │→ │  STT  │→ │ (LLM) │ │
│  │ (cpal)   │  │(silero│  │(sherpa│  │ resp) │ │
│  └──────────┘  │  onnx)│  │-onnx) │  └───┬───┘ │
│       ↑        └───────┘  └───────┘      │     │
│       │                                   ▼     │
│  ┌────┴─────┐                        ┌───────┐ │
│  │Echo Gate │←───────────────────────│  TTS  │ │
│  │(AtomicBool)                       │(sherpa│ │
│  └──────────┘                        │-onnx) │ │
│                                      └───┬───┘ │
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
| STT | Whisper (base.en default) | ONNX (tar.bz2 archive) | ~150 MB |
| TTS | Kokoro v0.19 | ONNX (tar.bz2 archive) | ~305 MB |
| VAD | Silero VAD v5 | ONNX (single file) | ~630 KB |

All models are downloaded on first voice activation from the
[k2-fsa/sherpa-onnx releases](https://github.com/k2-fsa/sherpa-onnx/releases)
and stored in `~/.local/share/gglib/voice_models/`.

## Backend

All speech processing uses [sherpa-rs](https://github.com/thewh1teagle/sherpa-rs)
(Rust bindings for sherpa-onnx) for STT, TTS, and VAD.

## Dependencies

Pre-built sherpa-onnx static libraries are downloaded at build
time — no system install of espeak-ng or other native libraries is required.
