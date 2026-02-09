import React, { createContext, useContext, useMemo } from 'react';

/**
 * Subset of UseVoiceModeReturn exposed to message bubble components.
 * Provides TTS speak action and relevant state for per-message controls.
 */
export interface VoiceContextValue {
  /** Speak text through TTS */
  speak: (text: string) => Promise<void>;
  /** Whether voice mode is currently active */
  isActive: boolean;
  /** Whether the TTS engine is loaded and ready */
  ttsLoaded: boolean;
  /** Whether TTS is currently playing audio */
  isSpeaking: boolean;
  /** Whether TTS audio is being generated (before playback starts) */
  isTtsGenerating: boolean;
}

const VoiceContext = createContext<VoiceContextValue | null>(null);

interface VoiceProviderProps {
  value: VoiceContextValue | null;
  children: React.ReactNode;
}

/**
 * Provides voice capabilities to message bubble components.
 * Renders a no-op provider (null value) when voice is unavailable.
 */
export function VoiceProvider({ value, children }: VoiceProviderProps) {
  return <VoiceContext.Provider value={value}>{children}</VoiceContext.Provider>;
}

/**
 * Access voice context from message bubble components.
 * Returns null when voice mode is not available (e.g. web mode).
 */
export function useVoiceContextOptional(): VoiceContextValue | null {
  return useContext(VoiceContext);
}

/**
 * Build a stable VoiceContextValue from voice hook fields.
 * Returns null if voice is not provided.
 */
export function useVoiceContextValue(voice?: {
  speak: (text: string) => Promise<void>;
  isActive: boolean;
  ttsLoaded: boolean;
  isSpeaking: boolean;
  isTtsGenerating: boolean;
}): VoiceContextValue | null {
  return useMemo<VoiceContextValue | null>(() => {
    if (!voice) return null;
    return {
      speak: voice.speak,
      isActive: voice.isActive,
      ttsLoaded: voice.ttsLoaded,
      isSpeaking: voice.isSpeaking,
      isTtsGenerating: voice.isTtsGenerating,
    };
  }, [voice?.speak, voice?.isActive, voice?.ttsLoaded, voice?.isSpeaking, voice?.isTtsGenerating]);
}
