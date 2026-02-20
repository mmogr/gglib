/**
 * Voice transport sub-interface.
 * Handles voice pipeline data/config operations (13 HTTP-backed methods).
 *
 * Audio I/O operations (start, stop, ptt, speak, stop-speaking) are NOT
 * included here — they remain Tauri-only until Phase 3.
 */

import type {
  VoiceStatusResponse,
  VoiceModelsResponse,
  AudioDeviceInfo,
  VoiceInteractionMode,
} from '../../../types/voice';

// Re-export domain types so consumers can import from the transport barrel
export type {
  VoiceState,
  VoiceInteractionMode,
  VoiceStatusResponse,
  SttModelInfo,
  TtsModelInfo,
  VoiceInfo,
  VoiceModelsResponse,
  AudioDeviceInfo,
  VoiceStatePayload,
  VoiceTranscriptPayload,
  VoiceAudioLevelPayload,
  VoiceErrorPayload,
  ModelDownloadProgressPayload,
} from '../../../types/voice';

/**
 * Voice transport operations (data/config — no audio I/O).
 *
 * All methods map 1-to-1 to the 13 HTTP endpoints exposed by `gglib-axum`.
 */
export interface VoiceTransport {
  /** GET /api/voice/status — current pipeline state. */
  voiceStatus(): Promise<VoiceStatusResponse>;

  /** GET /api/voice/models — available + downloaded STT/TTS/VAD models. */
  voiceListModels(): Promise<VoiceModelsResponse>;

  /** POST /api/voice/models/stt/{id}/download — download a specific STT model. */
  voiceDownloadSttModel(modelId: string): Promise<void>;

  /** POST /api/voice/models/tts/download — download the TTS model bundle. */
  voiceDownloadTtsModel(): Promise<void>;

  /** POST /api/voice/models/vad/download — download the VAD model. */
  voiceDownloadVadModel(): Promise<void>;

  /** POST /api/voice/stt/load — load a downloaded STT model into the pipeline. */
  voiceLoadStt(modelId: string): Promise<void>;

  /** POST /api/voice/tts/load — load the TTS model into the pipeline. */
  voiceLoadTts(): Promise<void>;

  /** PUT /api/voice/mode — switch between 'ptt' and 'vad' interaction modes. */
  voiceSetMode(mode: VoiceInteractionMode): Promise<void>;

  /** PUT /api/voice/voice — set the active TTS voice. */
  voiceSetVoice(voiceId: string): Promise<void>;

  /** PUT /api/voice/speed — set TTS playback speed. */
  voiceSetSpeed(speed: number): Promise<void>;

  /** PUT /api/voice/auto-speak — enable or disable automatic TTS after each response. */
  voiceSetAutoSpeak(autoSpeak: boolean): Promise<void>;

  /** POST /api/voice/unload — unload the pipeline, freeing STT/TTS model memory. */
  voiceUnload(): Promise<void>;

  /** GET /api/voice/devices — list available audio input/output devices. */
  voiceListDevices(): Promise<AudioDeviceInfo[]>;
}
