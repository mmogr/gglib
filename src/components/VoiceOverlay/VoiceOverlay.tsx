/**
 * VoiceOverlay — floating voice mode controls for the chat interface.
 *
 * Appears as a compact floating bar at the bottom of the chat when voice
 * mode is active. Shows current state, PTT button, audio level visualizer,
 * and quick controls.
 */

import { FC, useCallback, useEffect } from 'react';
import type { UseVoiceModeReturn } from '../../hooks/useVoiceMode';
import styles from './VoiceOverlay.module.css';

interface VoiceOverlayProps {
  voice: UseVoiceModeReturn;
  /** Callback when transcript is ready to send as a chat message */
  onTranscript?: (text: string) => void;
}

const STATE_LABELS: Record<string, string> = {
  idle: 'Voice Off',
  listening: 'Listening…',
  recording: 'Recording…',
  transcribing: 'Transcribing…',
  thinking: 'Thinking…',
  speaking: 'Speaking…',
  error: 'Error',
};

const STATE_ICONS: Record<string, string> = {
  idle: '🎙️',
  listening: '👂',
  recording: '🔴',
  transcribing: '💭',
  thinking: '🧠',
  speaking: '🔊',
  error: '⚠️',
};

export const VoiceOverlay: FC<VoiceOverlayProps> = ({ voice, onTranscript }) => {
  const {
    isSupported,
    isActive,
    voiceState,
    mode,
    isPttHeld,
    isSpeaking,
    audioLevel,
    lastTranscript,
    error,
    sttLoaded,
    ttsLoaded,
    pttStart,
    pttStop,
    stop,
    stopSpeaking,
    clearError,
  } = voice;

  // Forward transcripts to chat
  useEffect(() => {
    if (lastTranscript && onTranscript) {
      onTranscript(lastTranscript);
    }
  }, [lastTranscript, onTranscript]);

  // Keyboard shortcut: Space for PTT
  useEffect(() => {
    if (!isActive || mode !== 'ptt') return;

    const handleKeyDown = (e: KeyboardEvent) => {
      // Only trigger on Space when not in an input/textarea
      if (e.code !== 'Space') return;
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;
      if (e.repeat) return;

      e.preventDefault();
      pttStart();
    };

    const handleKeyUp = (e: KeyboardEvent) => {
      if (e.code !== 'Space') return;
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;

      e.preventDefault();
      pttStop();
    };

    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('keyup', handleKeyUp);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('keyup', handleKeyUp);
    };
  }, [isActive, mode, pttStart, pttStop]);

  const handlePttMouseDown = useCallback(() => {
    pttStart();
  }, [pttStart]);

  const handlePttMouseUp = useCallback(() => {
    pttStop();
  }, [pttStop]);

  // Don't render anything outside Tauri, or when voice mode is not active
  if (!isSupported || !isActive) return null;

  const stateLabel = STATE_LABELS[voiceState] ?? voiceState;
  const stateIcon = STATE_ICONS[voiceState] ?? '🎙️';
  const modelsReady = sttLoaded && ttsLoaded;

  return (
    <div className={styles.overlay}>
      {/* Status indicator */}
      <div className={styles.status}>
        <span className={styles.stateIcon}>{stateIcon}</span>
        <span className={styles.stateLabel}>{stateLabel}</span>
      </div>

      {/* Audio level visualizer */}
      <div className={styles.levelContainer}>
        <div
          className={styles.levelBar}
          style={{ width: `${Math.min(audioLevel * 100, 100)}%` }}
        />
      </div>

      {/* PTT button (only in PTT mode) */}
      {mode === 'ptt' && modelsReady && (
        <button
          className={`${styles.pttButton} ${isPttHeld ? styles.pttActive : ''}`}
          onMouseDown={handlePttMouseDown}
          onMouseUp={handlePttMouseUp}
          onMouseLeave={handlePttMouseUp}
          title="Hold to talk (or press Space)"
        >
          {isPttHeld ? '🔴 Release to send' : '🎙️ Hold to talk'}
        </button>
      )}

      {/* Stop speaking button */}
      {isSpeaking && (
        <button
          className={styles.controlButton}
          onClick={stopSpeaking}
          title="Stop speaking"
        >
          ⏹️ Stop
        </button>
      )}

      {/* Models not loaded warning */}
      {!modelsReady && (
        <span className={styles.warning}>
          Models not loaded — open Voice settings
        </span>
      )}

      {/* Error display */}
      {error && (
        <div className={styles.error}>
          <span>{error}</span>
          <button className={styles.dismissButton} onClick={clearError}>✕</button>
        </div>
      )}

      {/* Last transcript preview */}
      {lastTranscript && voiceState !== 'recording' && (
        <div className={styles.transcript} title={lastTranscript}>
          "{lastTranscript.length > 60
            ? lastTranscript.slice(0, 60) + '…'
            : lastTranscript}"
        </div>
      )}

      {/* Close voice mode */}
      <button
        className={styles.closeButton}
        onClick={stop}
        title="Close voice mode"
      >
        ✕
      </button>
    </div>
  );
};
