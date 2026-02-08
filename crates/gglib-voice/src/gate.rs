//! Echo gate — prevents TTS output from being captured by STT.
//!
//! When the system is speaking (TTS playback), the microphone input must be
//! suppressed to avoid the AI hearing its own voice and creating an infinite
//! conversation loop. This module provides a shared atomic flag for that purpose.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Shared echo gate that coordinates audio capture and playback.
///
/// When `is_system_speaking` is `true`:
/// - The capture module discards incoming audio samples
/// - The VAD module ignores all speech detection
///
/// The playback module sets this flag when it starts playing TTS audio,
/// and clears it when playback finishes or is interrupted.
#[derive(Debug, Clone)]
pub struct EchoGate {
    is_system_speaking: Arc<AtomicBool>,
}

impl EchoGate {
    /// Create a new echo gate (initially not speaking).
    #[must_use]
    pub fn new() -> Self {
        Self {
            is_system_speaking: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Mark the system as currently speaking (TTS playback active).
    ///
    /// While this is set, audio capture will be gated/suppressed.
    pub fn start_speaking(&self) {
        self.is_system_speaking.store(true, Ordering::SeqCst);
        tracing::debug!("Echo gate: system speaking — mic gated");
    }

    /// Mark the system as no longer speaking (TTS playback finished/stopped).
    ///
    /// Audio capture will resume.
    pub fn stop_speaking(&self) {
        self.is_system_speaking.store(false, Ordering::SeqCst);
        tracing::debug!("Echo gate: system silent — mic open");
    }

    /// Check whether the system is currently speaking.
    ///
    /// Used by capture and VAD modules to decide whether to process audio.
    #[must_use]
    pub fn is_speaking(&self) -> bool {
        self.is_system_speaking.load(Ordering::SeqCst)
    }
}

impl Default for EchoGate {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echo_gate_default_is_not_speaking() {
        let gate = EchoGate::new();
        assert!(!gate.is_speaking());
    }

    #[test]
    fn echo_gate_start_stop() {
        let gate = EchoGate::new();

        gate.start_speaking();
        assert!(gate.is_speaking());

        gate.stop_speaking();
        assert!(!gate.is_speaking());
    }

    #[test]
    fn echo_gate_clone_shares_state() {
        let gate1 = EchoGate::new();
        let gate2 = gate1.clone();

        gate1.start_speaking();
        assert!(gate2.is_speaking());

        gate2.stop_speaking();
        assert!(!gate1.is_speaking());
    }
}
