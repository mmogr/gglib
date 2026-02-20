/**
 * Voice HTTP API module.
 * Implements the 13 data/config voice operations via REST endpoints.
 *
 * Audio I/O operations (start, stop, ptt, speak) are NOT here — those
 * remain Tauri-only until Phase 3 (WebSocket audio bridge).
 *
 * @module services/transport/api/voice
 */

import { get, post, put } from './client';
import type {
  VoiceStatusResponse,
  VoiceModelsResponse,
  AudioDeviceInfo,
  VoiceInteractionMode,
} from '../../../types/voice';

export async function voiceStatus(): Promise<VoiceStatusResponse> {
  return get<VoiceStatusResponse>('/api/voice/status');
}

export async function voiceListModels(): Promise<VoiceModelsResponse> {
  return get<VoiceModelsResponse>('/api/voice/models');
}

export async function voiceDownloadSttModel(modelId: string): Promise<void> {
  return post<void>(`/api/voice/models/stt/${encodeURIComponent(modelId)}/download`);
}

export async function voiceDownloadTtsModel(): Promise<void> {
  return post<void>('/api/voice/models/tts/download');
}

export async function voiceDownloadVadModel(): Promise<void> {
  return post<void>('/api/voice/models/vad/download');
}

export async function voiceLoadStt(modelId: string): Promise<void> {
  return post<void>('/api/voice/stt/load', { modelId });
}

export async function voiceLoadTts(): Promise<void> {
  return post<void>('/api/voice/tts/load');
}

export async function voiceSetMode(mode: VoiceInteractionMode): Promise<void> {
  return put<void>('/api/voice/mode', { mode });
}

export async function voiceSetVoice(voiceId: string): Promise<void> {
  return put<void>('/api/voice/voice', { voiceId });
}

export async function voiceSetSpeed(speed: number): Promise<void> {
  return put<void>('/api/voice/speed', { speed });
}

export async function voiceSetAutoSpeak(autoSpeak: boolean): Promise<void> {
  return put<void>('/api/voice/auto-speak', { autoSpeak });
}

export async function voiceUnload(): Promise<void> {
  return post<void>('/api/voice/unload');
}

export async function voiceListDevices(): Promise<AudioDeviceInfo[]> {
  return get<AudioDeviceInfo[]>('/api/voice/devices');
}
