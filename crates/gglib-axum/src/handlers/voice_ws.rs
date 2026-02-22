//! WebSocket upgrade handler for the voice audio data plane.
//!
//! `GET /api/voice/audio` upgrades the connection to a binary WebSocket
//! that carries raw PCM audio between the browser and the server-side voice
//! pipeline.
//!
//! ## Protocol
//!
//! | Direction | Format | Rate | Channels | Frame size |
//! |---|---|---|---|---|
//! | Client → Server | PCM16 LE | 16 000 Hz | 1 (mono) | 960 bytes (30 ms) |
//! | Server → Client | PCM16 LE | 24 000 Hz | 1 (mono) | Variable |
//!
//! The WebSocket carries binary PCM frames for audio data and a single text
//! frame type for the playback-drained acknowledgement:
//!
//! | Direction | Type | Content |
//! |---|---|---|
//! | Client → Server | Binary, 960 bytes | PCM16 LE capture frame (30 ms) |
//! | Server → Client | Binary, variable | PCM16 LE playback frame |
//! | Client → Server | Text | `{"type":"playback_drained"}` |
//!
//! The `playback_drained` text frame is sent by the browser's `AudioWorklet`
//! main-thread handler when its ring buffer transitions from non-empty to
//! empty — i.e. all TTS PCM frames have been rendered.  The ingest task
//! recognises this sentinel and fires the pending completion callback stored
//! in [`WebSocketAudioSink`], causing the pipeline to emit the
//! `VoiceSpeakingFinished` SSE event at the precise moment the browser has
//! finished playing.  All voice lifecycle commands (`start`, `stop`,
//! `ptt-start`, …) continue to use the HTTP control-plane endpoints.
//!
//! ## Lifecycle
//!
//! 1. Browser opens `wss://…/api/voice/audio` (before calling `POST /api/voice/start`).
//! 2. Handler creates [`WebSocketAudioSource`] / [`WebSocketAudioSink`] channel pairs.
//! 3. Calls [`RemoteAudioRegistry::register_remote_audio`] so the next
//!    `POST /api/voice/start` uses the WS-backed source/sink instead of
//!    local cpal/rodio devices.
//! 4. Spawns two tasks:
//!    * **Ingest** — reads browser binary frames → decodes PCM16 LE → pushes
//!      `Vec<f32>` to the source channel.  Dropping the sender signals
//!      `AudioThreadDied` to the pipeline's VAD loop.
//!    * **Egress** — drains the sink channel → sends `Vec<u8>` (PCM16 LE)
//!      as binary WS frames to the browser.
//! 5. `tokio::select!` waits for either task to finish (graceful close or
//!    network drop).
//! 6. Calls [`RemoteAudioRegistry::deregister_remote_audio`] so a stale
//!    channel pair is never passed to a subsequent `start()`.

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use tracing::{info, warn};

use crate::state::AppState;
use crate::ws_audio::{WebSocketAudioSink, WebSocketAudioSource};

/// `GET /api/voice/audio` — WebSocket upgrade endpoint for audio data plane.
///
/// The browser must call this **before** `POST /api/voice/start`.  The handler
/// registers the channel-backed audio pair so the next `start()` uses
/// `WebSocketAudioSource`/`WebSocketAudioSink` instead of local cpal/rodio.
///
/// If `POST /api/voice/start` is called without an open WebSocket the
/// pipeline falls back to `LocalAudioSource`/`LocalAudioSink` (server
/// machine's mic and speakers — correct for the desktop app).
pub async fn audio_ws(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_audio_ws(socket, state))
}

async fn handle_audio_ws(socket: WebSocket, state: AppState) {
    // Create channel pairs.  The ingest task holds source_tx; dropping it
    // signals AudioThreadDied to the pipeline's VAD loop (clean shutdown).
    // The egress task holds sink_rx; the sink's frame_tx feeds it.
    let (source, source_tx) = WebSocketAudioSource::new();
    let (sink, sink_rx, drain_callbacks) = WebSocketAudioSink::new();

    // Register the pair — next VoicePipelinePort::start() will consume it.
    state
        .voice_registry
        .register_remote_audio(Box::new(source), Box::new(sink));

    info!("WebSocket audio session opened — remote audio registered");

    // Split the socket so the two tasks can use it concurrently.
    let (ws_sender, ws_receiver) = socket.split();

    // ── Ingest: browser PCM16 LE → decoded f32 → source channel ──────────

    let mut ingest = tokio::spawn(async move {
        let mut ws_receiver = ws_receiver;

        while let Some(msg_result) = ws_receiver.next().await {
            match msg_result {
                Ok(Message::Binary(data)) => {
                    // Validate frame length is even (each sample is 2 bytes).
                    if data.len() % 2 != 0 {
                        warn!(
                            bytes = data.len(),
                            "WS audio ingest: odd-length frame, skipping"
                        );
                        continue;
                    }

                    // Decode PCM16 LE (little-endian signed 16-bit) → f32.
                    let samples: Vec<f32> = data
                        .chunks_exact(2)
                        .map(|chunk| {
                            let i16_val = i16::from_le_bytes([chunk[0], chunk[1]]);
                            f32::from(i16_val) / 32_768.0
                        })
                        .collect();

                    if source_tx.send(samples).await.is_err() {
                        // Pipeline stopped — receiver dropped; exit cleanly.
                        break;
                    }
                }
                Ok(Message::Text(text)) => {
                    // `playback_drained`: the browser's AudioWorklet ring buffer
                    // has fully drained — all TTS PCM frames have been rendered.
                    // Fire the pending completion callback so the pipeline emits
                    // VoiceSpeakingFinished at the correct moment.
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        if json.get("type").and_then(|v| v.as_str())
                            == Some("playback_drained")
                        {
                            if let Some(cb) = drain_callbacks.lock().unwrap().take() {
                                cb();
                            }
                        } else {
                            warn!(msg = %text, "WS audio ingest: unexpected text frame");
                        }
                    }
                }
                // Graceful close or protocol error — stop ingest loop.
                Ok(Message::Close(_)) | Err(_) => break,
                // Ignore ping/pong frames.
                Ok(_) => {}
            }
        }
        // source_tx is dropped here.  WebSocketAudioSource::read_vad_frame()
        // will return Err(AudioThreadDied), causing the pipeline to self-stop.
    });

    // ── Egress: sink channel → PCM16 LE bytes → browser binary frames ────

    let mut egress = tokio::spawn(async move {
        let mut ws_sender = ws_sender;
        let mut sink_rx = sink_rx;

        while let Some(pcm_bytes) = sink_rx.recv().await {
            // Send as a binary WebSocket frame (Vec<u8> → Bytes via Into).
            if ws_sender
                .send(Message::Binary(pcm_bytes.into()))
                .await
                .is_err()
            {
                // Browser disconnected — exit silently.
                break;
            }
        }
    });

    // Wait for whichever task finishes first, then abort the other.
    // This covers both graceful WS close and abrupt network drops.
    tokio::select! {
        _ = &mut ingest => { egress.abort(); }
        _ = &mut egress => { ingest.abort(); }
    }

    // Always deregister — ensures a stale channel pair is never used by a
    // subsequent start() call after the browser reconnects.
    state.voice_registry.deregister_remote_audio();

    info!("WebSocket audio session closed — remote audio deregistered");
}
