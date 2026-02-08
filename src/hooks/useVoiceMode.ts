/**
 * useVoiceMode — React hook for voice conversation mode.
 *
 * Manages the voice pipeline lifecycle, event subscriptions, and
 * provides an imperative API for voice controls (PTT, speak, etc.).
 *
 * @module hooks/useVoiceMode
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import type { UnlistenFn } from '@tauri-apps/api/event';
import {
  isTauriEnvironment,
  voiceStart,
  voiceStop,
  voiceStatus,
  voicePttStart,
  voicePttStop,
  voiceSpeak,
  voiceStopSpeaking,
  voiceSetMode,
  voiceSetVoice,
  voiceSetSpeed,
  voiceSetAutoSpeak,
  voiceListDevices,
  voiceListModels,
  voiceDownloadSttModel,
  voiceDownloadTtsModel,
  voiceLoadStt,
  voiceLoadTts,
  onVoiceStateChanged,
  onVoiceTranscript,
  onVoiceSpeakingStarted,
  onVoiceSpeakingFinished,
  onVoiceAudioLevel,
  onVoiceError,
  onModelDownloadProgress,
} from '../services/clients/voice';
import type {
  VoiceState,
  VoiceInteractionMode,
  VoiceStatusResponse,
  VoiceModelsResponse,
  AudioDeviceInfo,
  ModelDownloadProgressPayload,
} from '../services/clients/voice';

// ── Hook return type ───────────────────────────────────────────────

export interface UseVoiceModeReturn {
  /** Whether voice mode is supported (Tauri environment) */
  isSupported: boolean;
  /** Whether voice mode is currently active */
  isActive: boolean;
  /** Current voice pipeline state */
  voiceState: VoiceState;
  /** Current interaction mode */
  mode: VoiceInteractionMode;
  /** Whether STT engine is loaded */
  sttLoaded: boolean;
  /** Whether TTS engine is loaded */
  ttsLoaded: boolean;
  /** Whether auto-speak is enabled */
  autoSpeak: boolean;
  /** Current microphone audio level (0-1) */
  audioLevel: number;
  /** Last transcribed text */
  lastTranscript: string | null;
  /** Whether PTT button is being held */
  isPttHeld: boolean;
  /** Whether TTS is currently speaking */
  isSpeaking: boolean;
  /** Current error message, if any */
  error: string | null;
  /** Available voice models */
  models: VoiceModelsResponse | null;
  /** Available audio input devices */
  devices: AudioDeviceInfo[];
  /** Model download progress */
  downloadProgress: ModelDownloadProgressPayload | null;
  /** Whether models are currently loading */
  modelsLoading: boolean;

  // Actions
  /** Start voice mode */
  start: (mode?: VoiceInteractionMode) => Promise<void>;
  /** Stop voice mode */
  stop: () => Promise<void>;
  /** Toggle voice mode on/off */
  toggle: () => Promise<void>;
  /** Start push-to-talk recording */
  pttStart: () => Promise<void>;
  /** Stop push-to-talk and transcribe */
  pttStop: () => Promise<string>;
  /** Speak text through TTS */
  speak: (text: string) => Promise<void>;
  /** Stop current TTS playback */
  stopSpeaking: () => Promise<void>;
  /** Change interaction mode */
  setMode: (mode: VoiceInteractionMode) => Promise<void>;
  /** Change TTS voice */
  setVoice: (voiceId: string) => Promise<void>;
  /** Change TTS speed */
  setSpeed: (speed: number) => Promise<void>;
  /** Toggle auto-speak */
  setAutoSpeak: (enabled: boolean) => Promise<void>;
  /** Download an STT model */
  downloadSttModel: (modelId: string) => Promise<void>;
  /** Download the TTS model */
  downloadTtsModel: () => Promise<void>;
  /** Load an STT model by ID */
  loadStt: (modelId: string) => Promise<void>;
  /** Load TTS model */
  loadTts: () => Promise<void>;
  /** Refresh available models and devices */
  refreshModels: () => Promise<void>;
  /** Clear the current error */
  clearError: () => void;
}

// ── Hook implementation ────────────────────────────────────────────

export function useVoiceMode(): UseVoiceModeReturn {
  const isSupported = isTauriEnvironment();

  // Pipeline state
  const [isActive, setIsActive] = useState(false);
  const [voiceState, setVoiceState] = useState<VoiceState>('idle');
  const [mode, setModeState] = useState<VoiceInteractionMode>('ptt');
  const [sttLoaded, setSttLoaded] = useState(false);
  const [ttsLoaded, setTtsLoaded] = useState(false);
  const [autoSpeak, setAutoSpeakState] = useState(true);

  // UI state
  const [audioLevel, setAudioLevel] = useState(0);
  const [lastTranscript, setLastTranscript] = useState<string | null>(null);
  const [isPttHeld, setIsPttHeld] = useState(false);
  const [isSpeaking, setIsSpeaking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Model/device state
  const [models, setModels] = useState<VoiceModelsResponse | null>(null);
  const [devices, setDevices] = useState<AudioDeviceInfo[]>([]);
  const [downloadProgress, setDownloadProgress] = useState<ModelDownloadProgressPayload | null>(null);
  const [modelsLoading, setModelsLoading] = useState(false);

  // Cleanup refs
  const unlistenRefs = useRef<UnlistenFn[]>([]);

  // ── Event subscriptions ────────────────────────────────────────

  const subscribeToEvents = useCallback(async () => {
    if (!isSupported) return;

    const unlisteners = await Promise.all([
      onVoiceStateChanged(({ state }) => {
        setVoiceState(state);
      }),
      onVoiceTranscript(({ text, isFinal }) => {
        if (isFinal) {
          setLastTranscript(text);
        }
      }),
      onVoiceSpeakingStarted(() => {
        setIsSpeaking(true);
      }),
      onVoiceSpeakingFinished(() => {
        setIsSpeaking(false);
      }),
      onVoiceAudioLevel(({ level }) => {
        setAudioLevel(level);
      }),
      onVoiceError(({ message }) => {
        setError(message);
      }),
      onModelDownloadProgress((progress) => {
        setDownloadProgress(progress);
        // Clear progress when complete
        if (progress.totalBytes && progress.bytesDownloaded >= progress.totalBytes) {
          setTimeout(() => setDownloadProgress(null), 1000);
        }
      }),
    ]);

    unlistenRefs.current = unlisteners;
  }, [isSupported]);

  // Subscribe on mount, cleanup on unmount
  useEffect(() => {
    subscribeToEvents();

    return () => {
      for (const unlisten of unlistenRefs.current) {
        unlisten();
      }
      unlistenRefs.current = [];
    };
  }, [subscribeToEvents]);

  // ── Sync status on mount ───────────────────────────────────────

  useEffect(() => {
    if (!isSupported) return;

    voiceStatus()
      .then((status: VoiceStatusResponse) => {
        setIsActive(status.isActive);
        setVoiceState(status.state as VoiceState);
        setModeState(status.mode as VoiceInteractionMode);
        setSttLoaded(status.sttLoaded);
        setTtsLoaded(status.ttsLoaded);
        setAutoSpeakState(status.autoSpeak);
      })
      .catch(() => {
        // Voice commands may not be available yet
      });
  }, [isSupported]);

  // ── Actions ────────────────────────────────────────────────────

  const start = useCallback(async (startMode?: VoiceInteractionMode) => {
    try {
      setError(null);
      await voiceStart(startMode);
      // Refresh full status from backend (models may have been preloaded)
      const status = await voiceStatus();
      setIsActive(status.isActive);
      setVoiceState(status.state as VoiceState);
      setModeState(status.mode as VoiceInteractionMode);
      setSttLoaded(status.sttLoaded);
      setTtsLoaded(status.ttsLoaded);
      setAutoSpeakState(status.autoSpeak);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const stop = useCallback(async () => {
    try {
      await voiceStop();
      setIsActive(false);
      setVoiceState('idle');
      setAudioLevel(0);
      setIsPttHeld(false);
      setIsSpeaking(false);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const toggle = useCallback(async () => {
    if (isActive) {
      await stop();
    } else {
      await start();
    }
  }, [isActive, start, stop]);

  const pttStart = useCallback(async () => {
    try {
      setError(null);
      setIsPttHeld(true);
      await voicePttStart();
    } catch (e) {
      setIsPttHeld(false);
      setError(String(e));
    }
  }, []);

  const pttStop = useCallback(async () => {
    try {
      setIsPttHeld(false);
      const text = await voicePttStop();
      if (text) setLastTranscript(text);
      return text;
    } catch (e) {
      setError(String(e));
      return '';
    }
  }, []);

  const speak = useCallback(async (text: string) => {
    try {
      setError(null);
      await voiceSpeak(text);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const stopSpeakingAction = useCallback(async () => {
    try {
      await voiceStopSpeaking();
      setIsSpeaking(false);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const setMode = useCallback(async (newMode: VoiceInteractionMode) => {
    try {
      await voiceSetMode(newMode);
      setModeState(newMode);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const setVoice = useCallback(async (voiceId: string) => {
    try {
      await voiceSetVoice(voiceId);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const setSpeed = useCallback(async (speed: number) => {
    try {
      await voiceSetSpeed(speed);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const setAutoSpeakAction = useCallback(async (enabled: boolean) => {
    try {
      await voiceSetAutoSpeak(enabled);
      setAutoSpeakState(enabled);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const downloadSttModelAction = useCallback(async (modelId: string) => {
    try {
      setError(null);
      await voiceDownloadSttModel(modelId);
    } catch (e) {
      setError(String(e));
      throw e;
    }
  }, []);

  const downloadTtsModelAction = useCallback(async () => {
    try {
      setError(null);
      await voiceDownloadTtsModel();
    } catch (e) {
      setError(String(e));
      throw e;
    }
  }, []);

  const loadStt = useCallback(async (modelId: string) => {
    try {
      setError(null);
      await voiceLoadStt(modelId);
      setSttLoaded(true);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const loadTts = useCallback(async () => {
    try {
      setError(null);
      await voiceLoadTts();
      setTtsLoaded(true);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const refreshModels = useCallback(async () => {
    if (!isSupported) return;
    try {
      setModelsLoading(true);
      const [modelsData, devicesData] = await Promise.all([
        voiceListModels(),
        voiceListDevices(),
      ]);
      setModels(modelsData);
      setDevices(devicesData);
    } catch (e) {
      setError(String(e));
    } finally {
      setModelsLoading(false);
    }
  }, [isSupported]);

  const clearError = useCallback(() => setError(null), []);

  return {
    isSupported,
    isActive,
    voiceState,
    mode,
    sttLoaded,
    ttsLoaded,
    autoSpeak,
    audioLevel,
    lastTranscript,
    isPttHeld,
    isSpeaking,
    error,
    models,
    devices,
    downloadProgress,
    modelsLoading,

    start,
    stop,
    toggle,
    pttStart,
    pttStop,
    speak,
    stopSpeaking: stopSpeakingAction,
    setMode,
    setVoice,
    setSpeed,
    setAutoSpeak: setAutoSpeakAction,
    downloadSttModel: downloadSttModelAction,
    downloadTtsModel: downloadTtsModelAction,
    loadStt,
    loadTts,
    refreshModels,
    clearError,
  };
}
