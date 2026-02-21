/**
 * Voice mode client.
 *
 * Data/config operations (status, model management, configuration) delegate
 * to the transport layer via getTransport() and work in both desktop and WebUI.
 *
 * Audio I/O operations (start, stop, ptt, speak) still call Tauri IPC directly
 * and remain desktop-only until Phase 3 (WebSocket audio bridge).
 *
 * @module services/clients/voice
 */

import type { Unsubscribe } from '../transport/types/common';
import type { VoiceEvent } from '../transport/types/events';
import { getTransport } from '../transport';
import type {
  VoiceState,
  VoiceInteractionMode,
  VoiceStatusResponse,
  SttModelInfo,
  TtsModelInfo,
  VoiceInfo,
  VoiceModelsResponse,
  AudioDeviceInfo,
  ModelDownloadProgressPayload,
} from '../../types/voice';

// Re-export all voice types so existing importers of this module keep working.
export type {
  VoiceState,
  VoiceInteractionMode,
  VoiceStatusResponse,
  SttModelInfo,
  TtsModelInfo,
  VoiceInfo,
  VoiceModelsResponse,
  AudioDeviceInfo,
  ModelDownloadProgressPayload,
};
// Re-export VoiceEvent so hooks can use it without reaching into transport internals.
export type { VoiceEvent };

// ── Tauri IPC helper (audio I/O only) ─────────────────────────────

async function invokeTauri<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  // @ts-expect-error - Tauri API injected at runtime
  const { invoke } = window.__TAURI_INTERNALS__;
  return invoke(cmd, args);
}

// ── Pipeline lifecycle ─────────────────────────────────────────────

export async function voiceStart(mode?: VoiceInteractionMode): Promise<void> {
  await invokeTauri('voice_start', mode ? { mode } : undefined);
}

export async function voiceStop(): Promise<void> {
  await invokeTauri('voice_stop');
}

/**
 * Fully unload the voice pipeline, freeing STT/TTS model memory.
 *
 * Use this when the user explicitly changes models or when memory must be
 * reclaimed. For simply pausing voice mode while keeping models warm,
 * use {@link voiceStop} instead.
 */
export async function voiceUnload(): Promise<void> {
  return getTransport().voiceUnload();
}

export async function voiceStatus(): Promise<VoiceStatusResponse> {
  return getTransport().voiceStatus();
}

// ── Push-to-talk ───────────────────────────────────────────────────

export async function voicePttStart(): Promise<void> {
  await invokeTauri('voice_ptt_start');
}

export async function voicePttStop(): Promise<string> {
  return invokeTauri('voice_ptt_stop');
}

// ── TTS ────────────────────────────────────────────────────────────

export async function voiceSpeak(text: string): Promise<void> {
  await invokeTauri('voice_speak', { text });
}

export async function voiceStopSpeaking(): Promise<void> {
  await invokeTauri('voice_stop_speaking');
}

// ── Model management ───────────────────────────────────────────────

export async function voiceListModels(): Promise<VoiceModelsResponse> {
  return getTransport().voiceListModels();
}

export async function voiceDownloadSttModel(modelId: string): Promise<void> {
  return getTransport().voiceDownloadSttModel(modelId);
}

export async function voiceDownloadTtsModel(): Promise<void> {
  return getTransport().voiceDownloadTtsModel();
}

export async function voiceDownloadVadModel(): Promise<void> {
  return getTransport().voiceDownloadVadModel();
}

export async function voiceLoadStt(modelId: string): Promise<void> {
  return getTransport().voiceLoadStt(modelId);
}

export async function voiceLoadTts(): Promise<void> {
  return getTransport().voiceLoadTts();
}

// ── Configuration ──────────────────────────────────────────────────

export async function voiceSetMode(mode: VoiceInteractionMode): Promise<void> {
  return getTransport().voiceSetMode(mode);
}

export async function voiceSetVoice(voiceId: string): Promise<void> {
  return getTransport().voiceSetVoice(voiceId);
}

export async function voiceSetSpeed(speed: number): Promise<void> {
  return getTransport().voiceSetSpeed(speed);
}

export async function voiceSetAutoSpeak(autoSpeak: boolean): Promise<void> {
  return getTransport().voiceSetAutoSpeak(autoSpeak);
}

// ── Device management ──────────────────────────────────────────────

export async function voiceListDevices(): Promise<AudioDeviceInfo[]> {
  return getTransport().voiceListDevices();
}

// ── Event subscriptions ────────────────────────────────────────────

/**
 * Subscribe to all voice pipeline events via the transport layer.
 *
 * Routes through SSE in the WebUI and through Tauri IPC on desktop.
 * No platform branching required — `getTransport().subscribe('voice', …)`
 * dispatches to the correct adapter automatically.
 *
 * @returns An unsubscribe function; call it to remove the listener.
 */
export function subscribeVoiceEvents(
  handler: (event: VoiceEvent) => void,
): Unsubscribe {
  return getTransport().subscribe('voice', handler);
}
