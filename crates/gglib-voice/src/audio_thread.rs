//! Dedicated audio I/O thread — isolates `!Send` audio resources from the async runtime.
//!
//! `cpal::Stream` (capture) and `rodio::OutputStream` (playback) are `!Send` on
//! some platforms. Rather than using `unsafe impl Send/Sync` on the pipeline, we
//! confine both types to a single OS thread and communicate via channels.
//!
//! The public [`AudioThreadHandle`] is the `Send + Sync` proxy that the pipeline
//! holds. It exposes the same logical operations as `AudioCapture` / `AudioPlayback`
//! but routes every call through an [`AudioCommand`] sent to the actor thread.

use std::sync::mpsc;
use std::thread;

use crate::capture::AudioCapture;
use crate::error::VoiceError;
use crate::gate::EchoGate;
use crate::playback::{AudioPlayback, PlaybackDoneCallback};

// ── Commands ───────────────────────────────────────────────────────

/// A command sent from the pipeline to the audio thread.
enum AudioCommand {
    /// Begin recording from the microphone.
    StartCapture {
        reply: mpsc::Sender<Result<(), VoiceError>>,
    },

    /// Stop recording and return the captured 16 kHz mono PCM samples.
    StopCapture {
        reply: mpsc::Sender<Result<Vec<f32>, VoiceError>>,
    },

    /// Query whether the microphone is currently recording.
    IsRecording { reply: mpsc::Sender<bool> },

    /// Prepare a streaming playback sink (echo gate activated).
    StartStreaming {
        reply: mpsc::Sender<Result<(), VoiceError>>,
    },

    /// Append audio samples to the current playback sink.
    Append {
        samples: Vec<f32>,
        sample_rate: u32,
        reply: mpsc::Sender<Result<(), VoiceError>>,
    },

    /// Stop any active playback immediately (fire-and-forget).
    StopPlayback,

    /// Query whether audio is currently playing.
    IsPlaying { reply: mpsc::Sender<bool> },

    /// Spawn a background watcher that fires `on_done` when the sink drains.
    SpawnCompletionWatcher {
        on_done: Option<PlaybackDoneCallback>,
    },

    /// Shut down the audio thread, releasing all resources.
    Shutdown,
}

// ── Handle (Send + Sync proxy) ─────────────────────────────────────

/// `Send + Sync` handle to the dedicated audio I/O thread.
///
/// `cpal::Stream` and `rodio::OutputStream` are `!Send` on some platforms
/// (macOS CoreAudio, etc.). This handle confines both types to a single OS
/// thread and proxies every operation through an `mpsc` channel, making the
/// pipeline naturally `Send + Sync` without any `unsafe` impls.
///
/// All methods take `&self` — the underlying `mpsc::Sender` supports shared
/// access. Request–reply methods block the caller until the audio thread
/// responds; this latency is negligible (microseconds of local channel I/O
/// plus the audio operation itself).
pub struct AudioThreadHandle {
    cmd_tx: mpsc::Sender<AudioCommand>,
    thread: Option<thread::JoinHandle<()>>,
}

impl AudioThreadHandle {
    /// Spawn the audio thread, initialise capture + playback, and return
    /// the handle.
    ///
    /// Errors from `AudioCapture::new` / `AudioPlayback::new` are propagated
    /// back to the caller via a one-shot init channel.
    pub fn spawn(echo_gate: EchoGate) -> Result<Self, VoiceError> {
        let (cmd_tx, cmd_rx) = mpsc::channel::<AudioCommand>();
        let (init_tx, init_rx) = mpsc::channel::<Result<(), VoiceError>>();

        let gate_clone = echo_gate.clone();

        let thread = thread::Builder::new()
            .name("gglib-audio".into())
            .spawn(move || {
                Self::run(gate_clone, cmd_rx, init_tx);
            })
            .map_err(|e| {
                VoiceError::InputStreamError(format!("failed to spawn audio thread: {e}"))
            })?;

        // Wait for the audio thread to finish initialisation.
        init_rx.recv().map_err(|_| VoiceError::AudioThreadDied)??;

        Ok(Self {
            cmd_tx,
            thread: Some(thread),
        })
    }

    // ── Capture ────────────────────────────────────────────────────

    /// Begin recording from the microphone.
    pub fn start_capture(&self) -> Result<(), VoiceError> {
        self.send_and_recv(|reply| AudioCommand::StartCapture { reply })
    }

    /// Stop recording and return captured 16 kHz mono PCM samples.
    pub fn stop_capture(&self) -> Result<Vec<f32>, VoiceError> {
        self.send_and_recv(|reply| AudioCommand::StopCapture { reply })
    }

    /// Check whether the microphone is currently recording.
    pub fn is_recording(&self) -> bool {
        self.query(|reply| AudioCommand::IsRecording { reply })
            .unwrap_or(false)
    }

    // ── Playback ───────────────────────────────────────────────────

    /// Prepare a streaming playback sink (echo gate activated).
    pub fn start_streaming(&self) -> Result<(), VoiceError> {
        self.send_and_recv(|reply| AudioCommand::StartStreaming { reply })
    }

    /// Append audio samples to the current playback sink.
    pub fn append(&self, samples: Vec<f32>, sample_rate: u32) -> Result<(), VoiceError> {
        self.send_and_recv(|reply| AudioCommand::Append {
            samples,
            sample_rate,
            reply,
        })
    }

    /// Stop any active playback immediately (fire-and-forget).
    pub fn stop_playback(&self) {
        let _ = self.cmd_tx.send(AudioCommand::StopPlayback);
    }

    /// Check whether audio is currently playing.
    pub fn is_playing(&self) -> bool {
        self.query(|reply| AudioCommand::IsPlaying { reply })
            .unwrap_or(false)
    }

    /// Spawn a background watcher that fires `on_done` when the sink drains.
    pub fn spawn_completion_watcher(&self, on_done: Option<PlaybackDoneCallback>) {
        let _ = self
            .cmd_tx
            .send(AudioCommand::SpawnCompletionWatcher { on_done });
    }

    // ── Internal helpers ───────────────────────────────────────────

    /// Send a command that expects a `Result<T, VoiceError>` reply. Creates
    /// a one-shot reply channel, sends the command, and blocks until the
    /// audio thread responds. Channel failures map to
    /// [`VoiceError::AudioThreadDied`].
    fn send_and_recv<T>(
        &self,
        build: impl FnOnce(mpsc::Sender<Result<T, VoiceError>>) -> AudioCommand,
    ) -> Result<T, VoiceError> {
        let (tx, rx) = mpsc::channel();
        self.cmd_tx
            .send(build(tx))
            .map_err(|_| VoiceError::AudioThreadDied)?;
        rx.recv().map_err(|_| VoiceError::AudioThreadDied)?
    }

    /// Like `send_and_recv` but for simple queries that return a bare value
    /// (no `Result` wrapper). Returns `None` if the thread is dead.
    fn query<T>(&self, build: impl FnOnce(mpsc::Sender<T>) -> AudioCommand) -> Option<T> {
        let (tx, rx) = mpsc::channel();
        self.cmd_tx.send(build(tx)).ok()?;
        rx.recv().ok()
    }

    // ── Audio thread event loop ────────────────────────────────────

    /// The body of the dedicated audio thread. Owns `AudioCapture` and
    /// `AudioPlayback` for their entire lifetime — they never cross thread
    /// boundaries.
    fn run(
        echo_gate: EchoGate,
        cmd_rx: mpsc::Receiver<AudioCommand>,
        init_tx: mpsc::Sender<Result<(), VoiceError>>,
    ) {
        // ── Initialise audio I/O on *this* thread ──────────────────
        let capture = match AudioCapture::new(echo_gate.clone()) {
            Ok(c) => c,
            Err(e) => {
                let _ = init_tx.send(Err(e));
                return;
            }
        };

        let playback = match AudioPlayback::new(echo_gate) {
            Ok(p) => p,
            Err(e) => {
                let _ = init_tx.send(Err(e));
                return;
            }
        };

        // Signal successful init.
        if init_tx.send(Ok(())).is_err() {
            // Caller dropped — nothing to do.
            return;
        }

        let mut capture = capture;
        let mut playback = playback;

        // ── Command loop (tight: recv → execute → reply → recv) ────
        while let Ok(cmd) = cmd_rx.recv() {
            match cmd {
                AudioCommand::StartCapture { reply } => {
                    let _ = reply.send(capture.start_recording());
                }

                AudioCommand::StopCapture { reply } => {
                    let _ = reply.send(capture.stop_recording());
                }

                AudioCommand::IsRecording { reply } => {
                    let _ = reply.send(capture.is_recording());
                }

                AudioCommand::StartStreaming { reply } => {
                    let _ = reply.send(playback.start_streaming());
                }

                AudioCommand::Append {
                    samples,
                    sample_rate,
                    reply,
                } => {
                    let _ = reply.send(playback.append(samples, sample_rate));
                }

                AudioCommand::StopPlayback => {
                    playback.stop();
                }

                AudioCommand::IsPlaying { reply } => {
                    let _ = reply.send(playback.is_playing());
                }

                AudioCommand::SpawnCompletionWatcher { on_done } => {
                    playback.spawn_completion_watcher(on_done);
                }

                AudioCommand::Shutdown => break,
            }
        }

        // `capture` and `playback` are dropped here, on the audio thread.
        tracing::debug!("Audio thread shutting down");
    }
}

impl Drop for AudioThreadHandle {
    fn drop(&mut self) {
        // Best-effort shutdown — the thread may already be dead.
        let _ = self.cmd_tx.send(AudioCommand::Shutdown);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}
