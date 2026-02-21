/**
 * Voice transport sub-interface.
 * Covers all 19 voice operations: 13 data/config + 6 audio I/O control.
 * Every method is backed by an HTTP endpoint in gglib-axum.
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
 * Voice transport operations.
 *
 * All 19 methods map 1-to-1 to HTTP endpoints exposed by `gglib-axum`.
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

  // ── Audio I/O control (Phase 3 / PR 2) ───────────────────────────────────

  /** POST /api/voice/start — activate the audio pipeline (optional mode override). */
  voiceStart(mode?: VoiceInteractionMode): Promise<void>;

  /** POST /api/voice/stop — pause the pipeline, keeping models loaded. */
  voiceStop(): Promise<void>;

  /** POST /api/voice/ptt-start — open the microphone for push-to-talk recording. */
  voicePttStart(): Promise<void>;

  /** POST /api/voice/ptt-stop — close PTT and return the transcript. */
  voicePttStop(): Promise<string>;

  /**
   * POST /api/voice/speak — synthesise and play back the given text.
   *
   * The server returns 202 Accepted immediately; listen for
   * `SpeakingStarted` / `SpeakingFinished` events on the SSE stream.
   */
  voiceSpeak(text: string): Promise<void>;

  /** POST /api/voice/stop-speaking — cancel in-progress TTS playback. */
  voiceStopSpeaking(): Promise<void>;
}
