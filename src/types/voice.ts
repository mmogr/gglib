/**
 * Voice domain types.
 *
 * These types are transport-agnostic data contracts shared between the
 * frontend service layer, transport interface, and UI components.
 * They must not reference any platform-specific (Tauri / HTTP) APIs.
 *
 * @module types/voice
 */

// ── Core state ─────────────────────────────────────────────────────

export type VoiceState =
  | 'idle'
  | 'listening'
  | 'recording'
  | 'transcribing'
  | 'thinking'
  | 'speaking'
  | 'error';

export type VoiceInteractionMode = 'ptt' | 'vad';

// ── Response shapes ────────────────────────────────────────────────

export interface VoiceStatusResponse {
  isActive: boolean;
  state: VoiceState;
  mode: VoiceInteractionMode;
  sttLoaded: boolean;
  ttsLoaded: boolean;
  /** ID of the currently loaded STT model, or null if none is loaded. */
  sttModelId: string | null;
  /** Currently configured TTS voice ID, or null if no pipeline exists. */
  ttsVoice: string | null;
  autoSpeak: boolean;
}

export interface SttModelInfo {
  id: string;
  name: string;
  sizeBytes: number;
  sizeDisplay: string;
  englishOnly: boolean;
  quality: number;
  speed: number;
  isDefault: boolean;
  /** Whether the model archive is already present on disk. */
  isDownloaded: boolean;
}

export interface TtsModelInfo {
  id: string;
  name: string;
  sizeBytes: number;
  sizeDisplay: string;
  voiceCount: number;
  /** Whether the model archive is already present on disk. */
  isDownloaded: boolean;
}

export interface VoiceInfo {
  id: string;
  name: string;
  category: string;
}

export interface VoiceModelsResponse {
  sttModels: SttModelInfo[];
  ttsModel: TtsModelInfo;
  voices: VoiceInfo[];
  vadDownloaded: boolean;
}

export interface AudioDeviceInfo {
  name: string;
  isDefault: boolean;
}

// ── Event payloads ─────────────────────────────────────────────────

export interface VoiceStatePayload {
  state: VoiceState;
}

export interface VoiceTranscriptPayload {
  text: string;
  isFinal: boolean;
}

export interface VoiceAudioLevelPayload {
  level: number;
}

export interface VoiceErrorPayload {
  message: string;
}

export interface ModelDownloadProgressPayload {
  modelId: string;
  bytesDownloaded: number;
  totalBytes: number;
  percent: number;
}
