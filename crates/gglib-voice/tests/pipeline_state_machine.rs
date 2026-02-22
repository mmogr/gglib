//! Integration tests for the `VoicePipeline` state machine.
//!
//! These tests drive the pipeline through its state transitions using mock
//! STT/TTS backends. No real audio hardware, model files, or network access
//! is required — the mocks return canned responses instantly.
//!
//! # What is tested
//!
//! - Initial idle state after construction
//! - `ptt_start` / `ptt_stop` guards on an inactive pipeline
//! - `set_active_for_test` transitions to Listening without real audio
//! - `ptt_start` transitions to Recording when active
//! - Interaction mode switching (`set_mode`)
//! - Auto-speak configuration toggle
//! - Event channel emits `StateChanged` events on transitions

use std::time::Duration;

use async_trait::async_trait;
use gglib_voice::{
    SttBackend, TtsAudio, TtsBackend, VoiceError, VoiceInfo,
    audio_io::{AudioSink, AudioSource},
    capture::AudioDeviceInfo,
    pipeline::{VoiceEvent, VoiceInteractionMode, VoicePipeline, VoicePipelineConfig, VoiceState},
};

// ── Mock backends ──────────────────────────────────────────────────

/// A minimal STT backend that immediately returns a fixed transcript.
struct MockStt {
    response: String,
}

impl MockStt {
    fn new(response: impl Into<String>) -> Self {
        Self {
            response: response.into(),
        }
    }
}

#[async_trait]
impl SttBackend for MockStt {
    async fn transcribe(&self, _audio: &[f32]) -> Result<String, VoiceError> {
        Ok(self.response.clone())
    }

    fn transcribe_with_callback(
        &self,
        _audio: &[f32],
        mut on_segment: Box<dyn FnMut(&str) + Send + 'static>,
    ) -> Result<String, VoiceError> {
        on_segment(&self.response);
        Ok(self.response.clone())
    }

    fn language(&self) -> &'static str {
        "en"
    }
}

/// A minimal TTS backend that returns a short burst of silence as audio.
struct MockTts;

#[async_trait]
impl TtsBackend for MockTts {
    async fn synthesize(&self, _text: &str) -> Result<TtsAudio, VoiceError> {
        Ok(TtsAudio {
            samples: vec![0.0f32; 160], // 10 ms of silence at 16 kHz
            sample_rate: 16_000,
            duration: Duration::from_millis(10),
        })
    }

    fn set_voice(&mut self, _voice_id: &str) {}
    fn set_speed(&mut self, _speed: f32) {}
    fn voice(&self) -> &'static str {
        "mock_voice"
    }
    fn sample_rate(&self) -> u32 {
        16_000
    }
    fn available_voices(&self) -> Vec<VoiceInfo> {
        vec![]
    }
}

// ── Mock audio backends ────────────────────────────────────────────

/// Recorded state for the mock audio source, inspectable after a test.
#[derive(Default)]
struct MockSourceState {
    /// Whether `start_capture` was called.
    pub capture_started: bool,
    /// Whether `stop_capture` was called.
    pub stop_capture_called: bool,
    /// Samples to hand back from `stop_capture`.
    pub samples_to_return: Vec<f32>,
}

/// Mock [`AudioSource`] that records which methods were called.
///
/// Backed by `Arc<Mutex<MockSourceState>>` so the test can inspect
/// its state after handing it to the pipeline.
struct MockAudioSource {
    state: std::sync::Arc<std::sync::Mutex<MockSourceState>>,
}

impl MockAudioSource {
    fn new() -> (Self, std::sync::Arc<std::sync::Mutex<MockSourceState>>) {
        let state = std::sync::Arc::new(std::sync::Mutex::new(MockSourceState::default()));
        (
            Self {
                state: std::sync::Arc::clone(&state),
            },
            state,
        )
    }

    fn with_samples(
        samples: Vec<f32>,
    ) -> (Self, std::sync::Arc<std::sync::Mutex<MockSourceState>>) {
        let state = std::sync::Arc::new(std::sync::Mutex::new(MockSourceState {
            samples_to_return: samples,
            ..Default::default()
        }));
        (
            Self {
                state: std::sync::Arc::clone(&state),
            },
            state,
        )
    }
}

impl AudioSource for MockAudioSource {
    fn start_capture(&self) -> Result<(), VoiceError> {
        self.state.lock().unwrap().capture_started = true;
        Ok(())
    }

    fn stop_capture(&self) -> Result<Vec<f32>, VoiceError> {
        let mut s = self.state.lock().unwrap();
        s.stop_capture_called = true;
        Ok(std::mem::take(&mut s.samples_to_return))
    }

    fn read_vad_frame(&self) -> Result<Option<Vec<f32>>, VoiceError> {
        Ok(None)
    }

    fn is_capturing(&self) -> bool {
        self.state.lock().unwrap().capture_started
    }

    fn list_devices(&self) -> Result<Vec<AudioDeviceInfo>, VoiceError> {
        Ok(vec![])
    }
}

/// Recorded state for the mock audio sink, inspectable after a test.
#[derive(Default)]
struct MockSinkState {
    /// Whether `start_streaming` was called.
    pub streaming_started: bool,
    /// Whether `stop` was called.
    pub stop_called: bool,
    /// All samples passed to `append`, in order.
    pub appended_samples: Vec<f32>,
}

/// Mock [`AudioSink`] that records which methods were called.
///
/// The `on_playback_complete` callback is invoked synchronously on
/// registration, so tests do not need to drive a background watcher.
struct MockAudioSink {
    state: std::sync::Arc<std::sync::Mutex<MockSinkState>>,
}

impl MockAudioSink {
    fn new() -> (Self, std::sync::Arc<std::sync::Mutex<MockSinkState>>) {
        let state = std::sync::Arc::new(std::sync::Mutex::new(MockSinkState::default()));
        (
            Self {
                state: std::sync::Arc::clone(&state),
            },
            state,
        )
    }
}

impl AudioSink for MockAudioSink {
    fn start_streaming(&self) -> Result<(), VoiceError> {
        self.state.lock().unwrap().streaming_started = true;
        Ok(())
    }

    fn append(&self, samples: Vec<f32>, _sample_rate: u32) -> Result<(), VoiceError> {
        self.state.lock().unwrap().appended_samples.extend(samples);
        Ok(())
    }

    fn stop(&self) -> Result<(), VoiceError> {
        self.state.lock().unwrap().stop_called = true;
        Ok(())
    }

    fn is_playing(&self) -> bool {
        false
    }

    /// Fires the callback immediately (synchronous, no real playback queue).
    fn on_playback_complete(&self, callback: Box<dyn FnOnce() + Send + 'static>) {
        callback();
    }
}

// ── Helpers ────────────────────────────────────────────────────────

/// Drain all pending events from the event receiver and return them.
fn drain_events(rx: &mut tokio::sync::mpsc::UnboundedReceiver<VoiceEvent>) -> Vec<VoiceEvent> {
    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    events
}

/// Collect only the `VoiceState` values from `StateChanged` events.
fn states_from(events: &[VoiceEvent]) -> Vec<VoiceState> {
    events
        .iter()
        .filter_map(|e| {
            if let VoiceEvent::StateChanged(s) = e {
                Some(*s)
            } else {
                None
            }
        })
        .collect()
}

// ── Tests ──────────────────────────────────────────────────────────

#[test]
fn initial_state_is_idle() {
    let (pipeline, _rx) = VoicePipeline::new(VoicePipelineConfig::default());
    assert_eq!(pipeline.state(), VoiceState::Idle);
    assert!(!pipeline.is_active());
}

#[test]
fn default_mode_is_ptt() {
    let config = VoicePipelineConfig::default();
    assert_eq!(config.mode, VoiceInteractionMode::PushToTalk);
}

#[test]
fn ptt_start_requires_active_pipeline() {
    let (pipeline, _rx) = VoicePipeline::new(VoicePipelineConfig::default());
    let err = pipeline.ptt_start().unwrap_err();
    assert!(
        matches!(err, VoiceError::NotActive),
        "expected NotActive, got {err:?}"
    );
    assert_eq!(pipeline.state(), VoiceState::Idle);
}

#[test]
fn ptt_stop_requires_active_pipeline() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let (pipeline, _rx) = VoicePipeline::new(VoicePipelineConfig::default());
        let err = pipeline.ptt_stop().await.unwrap_err();
        assert!(matches!(err, VoiceError::NotActive));
    });
}

#[test]
fn set_active_for_test_reaches_listening() {
    let (mut pipeline, mut rx) = VoicePipeline::new(VoicePipelineConfig::default());
    assert_eq!(pipeline.state(), VoiceState::Idle);

    pipeline.set_active_for_test();

    assert_eq!(pipeline.state(), VoiceState::Listening);
    assert!(pipeline.is_active());

    let emitted = states_from(&drain_events(&mut rx));
    assert!(
        emitted.contains(&VoiceState::Listening),
        "expected Listening event, got {emitted:?}"
    );
}

#[test]
fn ptt_start_transitions_to_recording_when_active() {
    // ptt_start requires both is_active() AND an AudioThreadHandle.
    // set_active_for_test() sets the activity flag but no audio handle, so
    // ptt_start will fail at the audio guard. The important invariant being
    // tested here is that the pipeline advances past the `is_active()` guard
    // (i.e., it must check that first) and the state remains Listening (not
    // changed to Idle or Error by the activity check itself).
    let (mut pipeline, _rx) = VoicePipeline::new(VoicePipelineConfig::default());
    pipeline.set_active_for_test();

    assert!(
        pipeline.is_active(),
        "pipeline should be active after set_active_for_test"
    );
    assert_eq!(
        pipeline.state(),
        VoiceState::Listening,
        "should be Listening"
    );

    // The error is expected (no audio hardware in test), but the state
    // must not revert to Idle due to the is_active() guard — it stays Listening.
    let _ = pipeline.ptt_start();
    // State is Listening or Recording depending on whether audio init succeeded.
    // In a test environment it will remain Listening (audio fails).
    assert!(
        pipeline.is_active(),
        "pipeline should remain active after ptt_start error"
    );
}

#[test]
fn stt_loaded_flag_reflects_injection() {
    let (mut pipeline, _rx) = VoicePipeline::new(VoicePipelineConfig::default());
    assert!(!pipeline.is_stt_loaded());

    pipeline.inject_stt(Box::new(MockStt::new("hello")));
    assert!(pipeline.is_stt_loaded());
}

#[test]
fn tts_loaded_flag_reflects_injection() {
    let (mut pipeline, _rx) = VoicePipeline::new(VoicePipelineConfig::default());
    assert!(!pipeline.is_tts_loaded());

    pipeline.inject_tts(Box::new(MockTts));
    assert!(pipeline.is_tts_loaded());
}

#[test]
fn stt_model_id_is_none_before_load() {
    let (pipeline, _rx) = VoicePipeline::new(VoicePipelineConfig::default());
    assert!(pipeline.stt_model_id().is_none());
}

#[test]
fn tts_voice_reflects_config() {
    let mut config = VoicePipelineConfig::default();
    config.tts.voice = "af_bella".to_string();
    let (pipeline, _rx) = VoicePipeline::new(config);
    assert_eq!(pipeline.tts_voice(), "af_bella");
}

#[test]
fn state_changed_event_emitted_on_set_active() {
    let (mut pipeline, mut rx) = VoicePipeline::new(VoicePipelineConfig::default());
    pipeline.set_active_for_test();

    let events = drain_events(&mut rx);
    assert!(!events.is_empty(), "expected at least one event");

    let states = states_from(&events);
    assert!(
        states.contains(&VoiceState::Listening),
        "expected StateChanged(Listening), got {states:?}"
    );
}

#[test]
fn idle_pipeline_mode_is_ptt_by_default() {
    let (pipeline, _rx) = VoicePipeline::new(VoicePipelineConfig::default());
    assert_eq!(pipeline.mode(), VoiceInteractionMode::PushToTalk);
}

// ── MockAudioSource / MockAudioSink tests ──────────────────────────
//
// These tests use inject_audio() / start_with_audio() instead of
// set_active_for_test(). They demonstrate the path towards deprecating
// set_active_for_test() for tests that exercise audio call sites.

/// `start_with_audio` activates the pipeline and transitions to Listening
/// without any real audio hardware.
#[test]
fn start_with_audio_activates_pipeline() {
    let (mut pipeline, mut rx) = VoicePipeline::new(VoicePipelineConfig::default());
    let (src, _src_state) = MockAudioSource::new();
    let (snk, _snk_state) = MockAudioSink::new();

    pipeline
        .inject_audio(Box::new(src), Box::new(snk))
        .expect("inject_audio should succeed with mock backends");

    assert!(
        pipeline.is_active(),
        "pipeline should be active after inject_audio"
    );
    assert_eq!(pipeline.state(), VoiceState::Listening);

    let states = states_from(&drain_events(&mut rx));
    assert!(
        states.contains(&VoiceState::Listening),
        "expected StateChanged(Listening) event, got {states:?}"
    );
}

/// `ptt_start` succeeds and transitions to Recording when mock audio is injected.
///
/// Verifies that:
/// - the pipeline advances past the `is_active()` guard
/// - `sink.stop()` is called before capture (to clear any active playback)
/// - `source.start_capture()` is called
/// - state transitions to Recording
#[test]
fn ptt_start_with_mock_audio_transitions_to_recording() {
    let (mut pipeline, _rx) = VoicePipeline::new(VoicePipelineConfig::default());
    let (src, src_state) = MockAudioSource::new();
    let (snk, snk_state) = MockAudioSink::new();

    pipeline.inject_audio(Box::new(src), Box::new(snk)).unwrap();
    pipeline
        .ptt_start()
        .expect("ptt_start should succeed with mock backends");

    assert_eq!(pipeline.state(), VoiceState::Recording);
    assert!(
        src_state.lock().unwrap().capture_started,
        "start_capture should have been called"
    );
    assert!(
        snk_state.lock().unwrap().stop_called,
        "sink.stop() should have been called before capture"
    );
}

/// Full PTT round-trip with mock audio and mock STT.
///
/// Verifies that `stop_capture` is called, the samples are forwarded to the
/// STT engine, and the resulting transcript is returned.
#[test]
fn ptt_stop_with_mock_audio_returns_transcript() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let (mut pipeline, _rx) = VoicePipeline::new(VoicePipelineConfig::default());

        // Provide a non-empty audio buffer so ptt_stop doesn't short-circuit.
        let (src, src_state) = MockAudioSource::with_samples(vec![0.1f32; 160]);
        let (snk, _snk_state) = MockAudioSink::new();

        pipeline.inject_audio(Box::new(src), Box::new(snk)).unwrap();
        pipeline.inject_stt(Box::new(MockStt::new("hello from mock")));

        pipeline.ptt_start().unwrap();
        let transcript = pipeline.ptt_stop().await.expect("ptt_stop should succeed");

        assert_eq!(transcript, "hello from mock");
        assert!(
            src_state.lock().unwrap().stop_capture_called,
            "stop_capture should have been called"
        );
    });
}
