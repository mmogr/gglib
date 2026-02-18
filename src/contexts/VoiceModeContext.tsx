/**
 * VoiceModeContext — app-level singleton for the voice mode hook.
 *
 * `useVoiceMode()` creates independent React state and Tauri event listeners
 * per call-site. Having two instances (ChatPage + VoiceSettings) mounted at
 * the same time causes duplicated listeners and subtly divergent state.
 *
 * This module provides a single `VoiceModeProvider` that should wrap the
 * app (inside `<SettingsProvider>` so settings defaults are available).
 * All components consume the shared instance via `useVoiceModeContext()`.
 *
 * @module contexts/VoiceModeContext
 */

import { createContext, useContext, type ReactNode } from 'react';
import { useVoiceMode, type UseVoiceModeReturn } from '../hooks/useVoiceMode';
import { useSettingsContext } from './SettingsContext';

const VoiceModeContext = createContext<UseVoiceModeReturn | null>(null);

interface VoiceModeProviderProps {
  children: ReactNode;
}

/**
 * App-level provider that creates a single `useVoiceMode()` instance.
 *
 * Must be rendered inside `<SettingsProvider>` so it can read persisted
 * voice defaults. Renders outside the Tauri environment too — the hook
 * gracefully returns `isSupported: false` in web/browser mode.
 */
export function VoiceModeProvider({ children }: VoiceModeProviderProps) {
  const { settings } = useSettingsContext();

  const voice = useVoiceMode({
    sttModel: settings?.voiceSttModel,
    ttsVoice: settings?.voiceTtsVoice,
    ttsSpeed: settings?.voiceTtsSpeed,
    interactionMode: settings?.voiceInteractionMode,
    autoSpeak: settings?.voiceAutoSpeak,
  });

  return (
    <VoiceModeContext.Provider value={voice}>
      {children}
    </VoiceModeContext.Provider>
  );
}

/**
 * Access the shared voice mode instance.
 *
 * Returns `null` when called outside `<VoiceModeProvider>` — this should
 * not happen in normal usage but provides graceful degradation instead of
 * a thrown error (e.g. if a component is rendered in a story or test
 * outside the full provider tree).
 */
export function useVoiceModeContext(): UseVoiceModeReturn | null {
  return useContext(VoiceModeContext);
}
