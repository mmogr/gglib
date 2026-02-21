/**
 * Voice HTTP API module.
 * Implements all 19 voice operations via REST endpoints.
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

// ── Audio I/O control (Phase 3 / PR 2) ───────────────────────────────────────

export async function voiceStart(mode?: VoiceInteractionMode): Promise<void> {
  return post<void>('/api/voice/start', mode ? { mode } : undefined);
}

export async function voiceStop(): Promise<void> {
  return post<void>('/api/voice/stop');
}

export async function voicePttStart(): Promise<void> {
  return post<void>('/api/voice/ptt-start');
}

export async function voicePttStop(): Promise<string> {
  const data = await post<{ transcript: string }>('/api/voice/ptt-stop');
  return data.transcript;
}

export async function voiceSpeak(text: string): Promise<void> {
  return post<void>('/api/voice/speak', { text });
}

export async function voiceStopSpeaking(): Promise<void> {
  return post<void>('/api/voice/stop-speaking');
}
