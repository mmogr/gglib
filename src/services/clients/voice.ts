/**
 * Voice mode client — Tauri-only commands for voice pipeline control.
 *
 * Voice mode is a desktop-only feature that requires native OS APIs
 * (microphone, audio playback). These commands bypass the HTTP transport
 * and call Tauri IPC directly.
 *
 * @module services/clients/voice
 */

import type { UnlistenFn } from '@tauri-apps/api/event';

// ── Types ──────────────────────────────────────────────────────────

export type VoiceState =
  | 'idle'
  | 'listening'
  | 'recording'
  | 'transcribing'
  | 'thinking'
  | 'speaking'
  | 'error';

export type VoiceInteractionMode = 'push_to_talk' | 'voice_activity_detection';

export interface VoiceStatusResponse {
  isActive: boolean;
  state: VoiceState;
  mode: VoiceInteractionMode;
  sttLoaded: boolean;
  ttsLoaded: boolean;
  autoSpeak: boolean;
}

export interface SttModelInfo {
  id: string;
  name: string;
  filename: string;
  url: string;
  sizeBytes: number;
  sizeDisplay: string;
  englishOnly: boolean;
  quality: number;
  speed: number;
  isDefault: boolean;
}

export interface TtsModelInfo {
  id: string;
  name: string;
  modelFilename: string;
  voicesFilename: string;
  modelUrl: string;
  voicesUrl: string;
  sizeBytes: number;
  sizeDisplay: string;
  voiceCount: number;
}

export interface VoiceInfo {
  id: string;
  name: string;
  category: string;
  gender: string;
}

export interface VoiceModelsResponse {
  sttModels: SttModelInfo[];
  sttDownloaded: string[];
  ttsModel: TtsModelInfo;
  ttsDownloaded: boolean;
  voices: VoiceInfo[];
}

export interface AudioDeviceInfo {
  name: string;
  isDefault: boolean;
}

// Event payloads
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

// ── Tauri IPC helper ───────────────────────────────────────────────

async function invokeTauri<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  // @ts-expect-error - Tauri API injected at runtime
  const { invoke } = window.__TAURI_INTERNALS__;
  return invoke(cmd, args);
}

/**
 * Check if running inside a Tauri desktop app.
 */
export function isTauriEnvironment(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

// ── Pipeline lifecycle ─────────────────────────────────────────────

export async function voiceStart(mode?: VoiceInteractionMode): Promise<void> {
  await invokeTauri('voice_start', mode ? { mode } : undefined);
}

export async function voiceStop(): Promise<void> {
  await invokeTauri('voice_stop');
}

export async function voiceStatus(): Promise<VoiceStatusResponse> {
  return invokeTauri('voice_status');
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
  return invokeTauri('voice_list_models');
}

export async function voiceDownloadSttModel(modelId: string): Promise<void> {
  await invokeTauri('voice_download_stt_model', { modelId });
}

export async function voiceDownloadTtsModel(): Promise<void> {
  await invokeTauri('voice_download_tts_model');
}

export async function voiceLoadStt(modelId: string): Promise<void> {
  await invokeTauri('voice_load_stt', { modelId });
}

export async function voiceLoadTts(): Promise<void> {
  await invokeTauri('voice_load_tts');
}

// ── Configuration ──────────────────────────────────────────────────

export async function voiceSetMode(mode: VoiceInteractionMode): Promise<void> {
  await invokeTauri('voice_set_mode', { mode });
}

export async function voiceSetVoice(voiceId: string): Promise<void> {
  await invokeTauri('voice_set_voice', { voiceId });
}

export async function voiceSetSpeed(speed: number): Promise<void> {
  await invokeTauri('voice_set_speed', { speed });
}

export async function voiceSetAutoSpeak(autoSpeak: boolean): Promise<void> {
  await invokeTauri('voice_set_auto_speak', { autoSpeak });
}

// ── Device management ──────────────────────────────────────────────

export async function voiceListDevices(): Promise<AudioDeviceInfo[]> {
  return invokeTauri('voice_list_devices');
}

// ── Event subscriptions ────────────────────────────────────────────

type VoiceEventHandler<T> = (payload: T) => void;

async function listenToVoiceEvent<T>(
  eventName: string,
  handler: VoiceEventHandler<T>,
): Promise<UnlistenFn> {
  const { listen } = await import('@tauri-apps/api/event');
  return listen<T>(eventName, (event) => handler(event.payload));
}

export function onVoiceStateChanged(handler: VoiceEventHandler<VoiceStatePayload>): Promise<UnlistenFn> {
  return listenToVoiceEvent('voice:state-changed', handler);
}

export function onVoiceTranscript(handler: VoiceEventHandler<VoiceTranscriptPayload>): Promise<UnlistenFn> {
  return listenToVoiceEvent('voice:transcript', handler);
}

export function onVoiceSpeakingStarted(handler: VoiceEventHandler<void>): Promise<UnlistenFn> {
  return listenToVoiceEvent('voice:speaking-started', handler);
}

export function onVoiceSpeakingFinished(handler: VoiceEventHandler<void>): Promise<UnlistenFn> {
  return listenToVoiceEvent('voice:speaking-finished', handler);
}

export function onVoiceAudioLevel(handler: VoiceEventHandler<VoiceAudioLevelPayload>): Promise<UnlistenFn> {
  return listenToVoiceEvent('voice:audio-level', handler);
}

export function onVoiceError(handler: VoiceEventHandler<VoiceErrorPayload>): Promise<UnlistenFn> {
  return listenToVoiceEvent('voice:error', handler);
}

export function onModelDownloadProgress(handler: VoiceEventHandler<ModelDownloadProgressPayload>): Promise<UnlistenFn> {
  return listenToVoiceEvent('voice:model-download-progress', handler);
}
