//! Audio capture module — microphone input via `cpal`.
//!
//! Captures audio from the default input device, resamples to 16 kHz mono
//! (the format required by whisper.cpp), and respects the echo gate.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use rubato::{FftFixedIn, Resampler as _};

use crate::error::VoiceError;
use crate::gate::EchoGate;

/// Target sample rate for whisper.cpp (16 kHz mono).
pub const WHISPER_SAMPLE_RATE: u32 = 16_000;

/// Audio capture handle.
///
/// Wraps a `cpal` input stream and accumulates PCM samples. The captured
/// audio is resampled to 16 kHz mono for direct consumption by whisper.
pub struct AudioCapture {
    /// The active cpal input stream (None when not recording).
    _stream: Option<Stream>,

    /// Shared buffer of captured samples (16 kHz mono f32).
    buffer: Arc<Mutex<Vec<f32>>>,

    /// Whether we are currently recording.
    is_recording: Arc<AtomicBool>,

    /// Echo gate — when the system is speaking, captured audio is discarded.
    echo_gate: EchoGate,

    /// The device sample rate (used for resampling).
    device_sample_rate: u32,

    /// Number of input channels from the device.
    device_channels: u16,
}

/// Information about an available audio input device.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDeviceInfo {
    /// Human-readable device name.
    pub name: String,
    /// Whether this is the system default input device.
    pub is_default: bool,
}

impl AudioCapture {
    /// Create a new audio capture instance using the default input device.
    pub fn new(echo_gate: EchoGate) -> Result<Self, VoiceError> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or(VoiceError::NoInputDevice)?;

        let config = device
            .default_input_config()
            .map_err(|e| VoiceError::InputStreamError(e.to_string()))?;

        let device_sample_rate = config.sample_rate().0;
        let device_channels = config.channels();

        tracing::info!(
            device = %device.name().unwrap_or_default(),
            sample_rate = device_sample_rate,
            channels = device_channels,
            "Audio capture initialized"
        );

        Ok(Self {
            _stream: None,
            buffer: Arc::new(Mutex::new(Vec::new())),
            is_recording: Arc::new(AtomicBool::new(false)),
            echo_gate,
            device_sample_rate,
            device_channels,
        })
    }

    /// Start recording audio from the microphone.
    ///
    /// Audio is accumulated in an internal buffer. Call [`stop_recording`]
    /// to retrieve the captured audio as 16 kHz mono PCM samples.
    pub fn start_recording(&mut self) -> Result<(), VoiceError> {
        if self.is_recording.load(Ordering::SeqCst) {
            return Ok(()); // Already recording
        }

        // Clear the buffer for a fresh recording
        if let Ok(mut buf) = self.buffer.lock() {
            buf.clear();
        }

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or(VoiceError::NoInputDevice)?;

        let config = device
            .default_input_config()
            .map_err(|e| VoiceError::InputStreamError(e.to_string()))?;

        let stream = self.build_input_stream(&device, &config)?;
        stream
            .play()
            .map_err(|e| VoiceError::InputStreamError(e.to_string()))?;

        self._stream = Some(stream);
        self.is_recording.store(true, Ordering::SeqCst);
        tracing::debug!("Audio recording started");

        Ok(())
    }

    /// Stop recording and return the captured audio as 16 kHz mono f32 PCM.
    pub fn stop_recording(&mut self) -> Result<Vec<f32>, VoiceError> {
        self.is_recording.store(false, Ordering::SeqCst);

        // Drop the stream to stop capturing
        self._stream = None;

        let raw_samples = {
            let mut buf = self
                .buffer
                .lock()
                .map_err(|e| VoiceError::InputStreamError(e.to_string()))?;
            std::mem::take(&mut *buf)
        };

        tracing::debug!(
            raw_samples = raw_samples.len(),
            device_rate = self.device_sample_rate,
            "Audio recording stopped, resampling to 16kHz mono"
        );

        // Convert to mono if needed
        let mono = if self.device_channels > 1 {
            stereo_to_mono(&raw_samples, self.device_channels)
        } else {
            raw_samples
        };

        // Resample to 16 kHz if device sample rate differs
        if self.device_sample_rate == WHISPER_SAMPLE_RATE {
            Ok(mono)
        } else {
            resample(&mono, self.device_sample_rate, WHISPER_SAMPLE_RATE)
        }
    }

    /// Check if currently recording.
    #[must_use]
    pub fn is_recording(&self) -> bool {
        self.is_recording.load(Ordering::SeqCst)
    }

    /// List available audio input devices.
    pub fn list_devices() -> Result<Vec<AudioDeviceInfo>, VoiceError> {
        let host = cpal::default_host();
        let default_name = host
            .default_input_device()
            .and_then(|d| d.name().ok())
            .unwrap_or_default();

        let devices = host
            .input_devices()
            .map_err(|e| VoiceError::InputStreamError(e.to_string()))?;

        let mut result = Vec::new();
        for device in devices {
            if let Ok(name) = device.name() {
                result.push(AudioDeviceInfo {
                    is_default: name == default_name,
                    name,
                });
            }
        }

        Ok(result)
    }

    /// Build a cpal input stream that writes samples into the shared buffer.
    fn build_input_stream(
        &self,
        device: &Device,
        config: &cpal::SupportedStreamConfig,
    ) -> Result<Stream, VoiceError> {
        let buffer = Arc::clone(&self.buffer);
        let is_recording = Arc::clone(&self.is_recording);
        let echo_gate = self.echo_gate.clone();

        let stream_config: StreamConfig = config.clone().into();
        let sample_format = config.sample_format();

        let err_fn = |err: cpal::StreamError| {
            tracing::error!(%err, "Audio input stream error");
        };

        let stream = match sample_format {
            SampleFormat::F32 => device.build_input_stream(
                &stream_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if !is_recording.load(Ordering::Relaxed) {
                        return;
                    }
                    // Echo gate: discard audio when TTS is playing
                    if echo_gate.is_speaking() {
                        return;
                    }
                    if let Ok(mut buf) = buffer.lock() {
                        buf.extend_from_slice(data);
                    }
                },
                err_fn,
                None,
            ),
            SampleFormat::I16 => device.build_input_stream(
                &stream_config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    if !is_recording.load(Ordering::Relaxed) {
                        return;
                    }
                    if echo_gate.is_speaking() {
                        return;
                    }
                    // Convert i16 → f32
                    let float_data: Vec<f32> =
                        data.iter().map(|&s| f32::from(s) / 32768.0).collect();
                    if let Ok(mut buf) = buffer.lock() {
                        buf.extend_from_slice(&float_data);
                    }
                },
                err_fn,
                None,
            ),
            SampleFormat::I32 => device.build_input_stream(
                &stream_config,
                move |data: &[i32], _: &cpal::InputCallbackInfo| {
                    if !is_recording.load(Ordering::Relaxed) {
                        return;
                    }
                    if echo_gate.is_speaking() {
                        return;
                    }
                    #[allow(clippy::cast_precision_loss)]
                    let float_data: Vec<f32> =
                        data.iter().map(|&s| s as f32 / 2_147_483_648.0).collect();
                    if let Ok(mut buf) = buffer.lock() {
                        buf.extend_from_slice(&float_data);
                    }
                },
                err_fn,
                None,
            ),
            _ => {
                return Err(VoiceError::InputStreamError(format!(
                    "Unsupported sample format: {sample_format:?}"
                )));
            }
        };

        stream.map_err(|e| VoiceError::InputStreamError(e.to_string()))
    }
}

/// Convert interleaved multi-channel audio to mono by averaging channels.
fn stereo_to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    let channels = channels as usize;
    samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Resample audio from one sample rate to another using FFT-based resampling.
fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Result<Vec<f32>, VoiceError> {
    if samples.is_empty() {
        return Ok(Vec::new());
    }

    let chunk_size = 1024;

    let mut resampler = FftFixedIn::<f32>::new(
        from_rate as usize,
        to_rate as usize,
        chunk_size,
        2, // sub-chunks for quality
        1, // mono
    )
    .map_err(|e| VoiceError::ResampleError(e.to_string()))?;

    let mut output = Vec::new();

    // Process in chunks
    let mut pos = 0;
    while pos + chunk_size <= samples.len() {
        let chunk = &samples[pos..pos + chunk_size];
        let result = resampler
            .process(&[chunk], None)
            .map_err(|e| VoiceError::ResampleError(e.to_string()))?;
        if let Some(channel) = result.first() {
            output.extend_from_slice(channel);
        }
        pos += chunk_size;
    }

    // Handle remaining samples by padding with zeros
    if pos < samples.len() {
        let remaining = &samples[pos..];
        let mut padded = vec![0.0f32; chunk_size];
        padded[..remaining.len()].copy_from_slice(remaining);

        let result = resampler
            .process(&[&padded], None)
            .map_err(|e| VoiceError::ResampleError(e.to_string()))?;
        if let Some(channel) = result.first() {
            // Only take the proportional amount of output
            #[allow(clippy::cast_precision_loss)]
            let output_len =
                (remaining.len() as f64 * to_rate as f64 / from_rate as f64).ceil() as usize;
            let take = output_len.min(channel.len());
            output.extend_from_slice(&channel[..take]);
        }
    }

    Ok(output)
}
