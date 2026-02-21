/**
 * Voice mode client.
 *
 * All operations delegate to the transport layer via getTransport() and
 * work in both desktop and WebUI.  No platform branching required.
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

// ── Pipeline lifecycle ─────────────────────────────────────────────

export async function voiceStart(mode?: VoiceInteractionMode): Promise<void> {
  return getTransport().voiceStart(mode);
}

export async function voiceStop(): Promise<void> {
  return getTransport().voiceStop();
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
  return getTransport().voicePttStart();
}

export async function voicePttStop(): Promise<string> {
  return getTransport().voicePttStop();
}

// ── TTS ────────────────────────────────────────────────────────────

export async function voiceSpeak(text: string): Promise<void> {
  return getTransport().voiceSpeak(text);
}

export async function voiceStopSpeaking(): Promise<void> {
  return getTransport().voiceStopSpeaking();
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
