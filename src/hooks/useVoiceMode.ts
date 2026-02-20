/**
 * useVoiceMode — React hook for voice conversation mode.
 *
 * Manages the voice pipeline lifecycle, event subscriptions, and
 * provides an imperative API for voice controls (PTT, speak, etc.).
 *
 * When `start()` is called, the hook automatically loads persisted
 * default models (STT model, TTS voice/speed) from the optional
 * `defaults` config before starting the pipeline, so the user never
 * has to manually re‑select models after the initial download.
 *
 * @module hooks/useVoiceMode
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import type { UnlistenFn } from '@tauri-apps/api/event';
import {
  voiceStart,
  voiceStop,
  voiceUnload,
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
  voiceDownloadVadModel,
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

// ── Defaults from persisted settings ───────────────────────────────

/** Persisted defaults passed from SettingsContext / ChatPage. */
export interface VoiceDefaults {
  /** Default STT model id (e.g. "base.en") */
  sttModel?: string | null;
  /** Default TTS voice id (e.g. "af_sarah") */
  ttsVoice?: string | null;
  /** Default TTS speed multiplier */
  ttsSpeed?: number | null;
  /** Default interaction mode */
  interactionMode?: string | null;
  /** Default auto-speak preference */
  autoSpeak?: boolean | null;
}

// ── Hook return type ───────────────────────────────────────────────

export interface UseVoiceModeReturn {
  /** Whether voice mode is supported for data/config operations (always true — served via HTTP). */
  isSupported: boolean;
  /** Whether audio I/O is supported (Tauri desktop only until Phase 3). */
  isAudioSupported: boolean;
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
  /** Whether TTS audio is being generated (before playback starts) */
  isTtsGenerating: boolean;
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
  /** Whether default models are being auto-loaded on first activation */
  isAutoLoading: boolean;

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
  /** Download the VAD model (Silero) */
  downloadVadModel: () => Promise<void>;
  /** Load an STT model by ID */
  loadStt: (modelId: string) => Promise<void>;
  /** Load TTS model */
  loadTts: () => Promise<void>;
  /** Refresh available models and devices */
  refreshModels: () => Promise<void>;
  /** Clear the current error */
  clearError: () => void;
  /**
   * Fully unload the voice pipeline, freeing STT/TTS model memory.
   *
   * Use this when the user switches models or when memory must be reclaimed.
   * For simply pausing voice mode while keeping models warm, use `stop()`.
   */
  unload: () => Promise<void>;
}

// ── Hook implementation ────────────────────────────────────────────

/** Returns true when running inside a Tauri desktop app. */
function isTauriEnvironment(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export function useVoiceMode(defaults?: VoiceDefaults): UseVoiceModeReturn {
  // Data/config ops (status, models, config) work everywhere via HTTP.
  const isSupported = true;
  // Audio I/O (start/stop/ptt/speak) still requires Tauri until Phase 3.
  const isAudioSupported = isTauriEnvironment();

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
  const [isTtsGenerating, setIsTtsGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Model/device state
  const [models, setModels] = useState<VoiceModelsResponse | null>(null);
  const [devices, setDevices] = useState<AudioDeviceInfo[]>([]);
  const [downloadProgress, setDownloadProgress] = useState<ModelDownloadProgressPayload | null>(null);
  const [modelsLoading, setModelsLoading] = useState(false);
  const [isAutoLoading, setIsAutoLoading] = useState(false);

  // Cleanup refs
  const unlistenRefs = useRef<UnlistenFn[]>([]);

  // Race-condition guard: monotonically increasing load generation.
  // When a new load is requested, the generation advances; any
  // in-flight load with an older generation bails out before
  // mutating state (prevents memory conflicts from Step 8/9).
  const loadGenRef = useRef(0);

  // Keep defaults in a ref so the `start` callback always sees the
  // latest values without re-creating the closure on every render.
  const defaultsRef = useRef(defaults);
  defaultsRef.current = defaults;

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
        setIsTtsGenerating(false);
        setIsSpeaking(true);
      }),
      onVoiceSpeakingFinished(() => {
        setIsSpeaking(false);
        setIsTtsGenerating(false);
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

      // Release the microphone if voice is still active when this component
      // unmounts (e.g. user navigates away). voiceStop pauses the pipeline
      // and drops the audio thread (OS mic indicator off) but keeps loaded
      // models warm so the next start() is instant.
      voiceStatus()
        .then((status) => {
          if (status.isActive) {
            return voiceStop();
          }
        })
        .catch(() => {
          // Best-effort: ignore errors during unmount cleanup.
        });
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

      // Bump load generation so any in-flight load can detect staleness.
      const gen = ++loadGenRef.current;

      // Check what's already loaded on the backend pipeline.
      let status: VoiceStatusResponse;
      try {
        status = await voiceStatus();
      } catch {
        // Pipeline doesn't exist yet — treat as nothing loaded.
        status = {
          isActive: false,
          state: 'idle',
          mode: 'ptt',
          sttLoaded: false,
          ttsLoaded: false,
          sttModelId: null,
          ttsVoice: null,
          autoSpeak: true,
        };
      }

      const defs = defaultsRef.current;
      const needStt = !status.sttLoaded && !!defs?.sttModel;

      // Only attempt TTS auto-load if the model is actually downloaded.
      // Query the catalog to check; swallow errors (best-effort).
      let ttsDownloaded = false;
      try {
        const catalog = await voiceListModels();
        ttsDownloaded = catalog.ttsDownloaded;
      } catch { /* fall through — skip TTS auto-load */ }
      const needTts = !status.ttsLoaded && ttsDownloaded;

      // Show an auto-loading overlay immediately so the user sees
      // feedback while models are being loaded into memory.
      if (needStt || needTts) {
        setIsAutoLoading(true);
      }

      // Auto-load models from persisted defaults (lazy-load on first
      // activation). Each load is best-effort: if a model fails to
      // load the pipeline will still start (just without that engine).
      // Each load is also guarded by the generation counter to handle
      // race conditions when the user changes defaults mid-load.
      try {
        const loadPromises: Promise<void>[] = [];

        if (needStt && defs?.sttModel) {
          const sttId = defs.sttModel;
          loadPromises.push(
            voiceLoadStt(sttId)
              .then(() => {
                if (loadGenRef.current === gen) setSttLoaded(true);
              })
              .catch((e) => {
                // STT load failed — surface the error but don't block start.
                if (loadGenRef.current === gen) {
                  setError(`STT load failed: ${String(e)}`);
                }
              }),
          );
        }
        if (needTts) {
          loadPromises.push(
            voiceLoadTts()
              .then(() => {
                if (loadGenRef.current === gen) setTtsLoaded(true);
              })
              .catch((e) => {
                // TTS load failed — surface the error but don't block start.
                if (loadGenRef.current === gen) {
                  setError(`TTS load failed: ${String(e)}`);
                }
              }),
          );
        }

        // Wait for model loads to settle (all are best-effort).
        if (loadPromises.length > 0) {
          await Promise.all(loadPromises);
        }

        // Bail out if a newer start/load was triggered while we waited.
        if (loadGenRef.current !== gen) return;

        // Apply voice / speed / mode defaults to the pipeline.
        if (defs?.ttsVoice) {
          await voiceSetVoice(defs.ttsVoice).catch(() => {});
        }
        if (defs?.ttsSpeed != null) {
          await voiceSetSpeed(defs.ttsSpeed).catch(() => {});
        }
        if (defs?.autoSpeak != null) {
          await voiceSetAutoSpeak(defs.autoSpeak).catch(() => {});
          setAutoSpeakState(defs.autoSpeak);
        }
      } finally {
        if (loadGenRef.current === gen) setIsAutoLoading(false);
      }

      // Bail out again after async gap.
      if (loadGenRef.current !== gen) return;

      // Actually start the pipeline (audio I/O).
      await voiceStart(startMode);

      // Refresh full status from backend (models may have been preloaded)
      const finalStatus = await voiceStatus();
      setIsActive(finalStatus.isActive);
      setVoiceState(finalStatus.state as VoiceState);
      setModeState(finalStatus.mode as VoiceInteractionMode);
      setSttLoaded(finalStatus.sttLoaded);
      setTtsLoaded(finalStatus.ttsLoaded);
      setAutoSpeakState(finalStatus.autoSpeak);
    } catch (e) {
      setIsAutoLoading(false);
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
      setIsTtsGenerating(false);
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
      setIsTtsGenerating(true);
      await voiceSpeak(text);
    } catch (e) {
      setIsTtsGenerating(false);
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

  const downloadVadModelAction = useCallback(async () => {
    try {
      setError(null);
      await voiceDownloadVadModel();
    } catch (e) {
      setError(String(e));
      throw e;
    }
  }, []);

  const loadStt = useCallback(async (modelId: string) => {
    try {
      setError(null);
      // Bump generation to cancel any concurrent/previous load.
      const gen = ++loadGenRef.current;
      await voiceLoadStt(modelId);
      if (loadGenRef.current === gen) setSttLoaded(true);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const loadTts = useCallback(async () => {
    try {
      setError(null);
      const gen = ++loadGenRef.current;
      await voiceLoadTts();
      if (loadGenRef.current === gen) setTtsLoaded(true);
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

  const unload = useCallback(async () => {
    try {
      await voiceUnload();
      setIsActive(false);
      setVoiceState('idle');
      setSttLoaded(false);
      setTtsLoaded(false);
      setAudioLevel(0);
      setIsPttHeld(false);
      setIsSpeaking(false);
      setIsTtsGenerating(false);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  return {
    isSupported,
    isAudioSupported,
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
    isTtsGenerating,
    error,
    models,
    devices,
    downloadProgress,
    modelsLoading,
    isAutoLoading,

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
    downloadVadModel: downloadVadModelAction,
    loadStt,
    loadTts,
    refreshModels,
    clearError,
    unload,
  };
}
