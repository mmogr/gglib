/**
 * VoiceOverlay — floating voice mode controls for the chat interface.
 *
 * Appears as a compact floating bar at the bottom of the chat when voice
 * mode is active. Shows current state, PTT button, audio level visualizer,
 * and quick controls.
 */

import { FC, useCallback, useEffect } from 'react';
import type { UseVoiceModeReturn } from '../../hooks/useVoiceMode';
import { cn } from '../../utils/cn';

interface VoiceOverlayProps {
  /** Shared voice mode instance. Passing null/undefined renders nothing. */
  voice: UseVoiceModeReturn | null | undefined;
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
  const isAudioSupported = voice?.isAudioSupported ?? false;
  const isActive = voice?.isActive ?? false;
  const voiceState = voice?.voiceState ?? 'idle';
  const mode = voice?.mode ?? 'ptt';
  const isPttHeld = voice?.isPttHeld ?? false;
  const isSpeaking = voice?.isSpeaking ?? false;
  const isTtsGenerating = voice?.isTtsGenerating ?? false;
  const audioLevel = voice?.audioLevel ?? 0;
  const lastTranscript = voice?.lastTranscript ?? null;
  const error = voice?.error ?? null;
  const sttLoaded = voice?.sttLoaded ?? false;
  const ttsLoaded = voice?.ttsLoaded ?? false;
  const pttStart = voice?.pttStart;
  const pttStop = voice?.pttStop;
  const stop = voice?.stop;
  const stopSpeaking = voice?.stopSpeaking;
  const clearError = voice?.clearError;

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
      pttStart?.();
    };

    const handleKeyUp = (e: KeyboardEvent) => {
      if (e.code !== 'Space') return;
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;

      e.preventDefault();
      pttStop?.();
    };

    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('keyup', handleKeyUp);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('keyup', handleKeyUp);
    };
  }, [isActive, mode, pttStart, pttStop]);

  const handlePttMouseDown = useCallback(() => {
    pttStart?.();
  }, [pttStart]);

  const handlePttMouseUp = useCallback(() => {
    pttStop?.();
  }, [pttStop]);

  // Don't render anything outside Tauri or when voice is unavailable.
  // Render during auto-loading even before the pipeline is fully active.
  const isAutoLoading = voice?.isAutoLoading ?? false;
  if (!isAudioSupported || (!isActive && !isAutoLoading)) return null;

  const stateLabel = STATE_LABELS[voiceState] ?? voiceState;
  const stateIcon = STATE_ICONS[voiceState] ?? '🎙️';
  const modelsReady = sttLoaded && ttsLoaded;
  const showAutoLoading = isAutoLoading;

  return (
    <div className="fixed bottom-lg left-1/2 -translate-x-1/2 flex items-center gap-sm px-md py-sm bg-surface border border-border rounded-lg shadow-[0_4px_24px_rgba(0,0,0,0.3)] z-[1000] min-w-[320px] max-w-[600px] backdrop-blur-[8px]">
      {/* Status indicator */}
      <div className="flex items-center gap-xs shrink-0">
        <span className="text-[1.1em]">{stateIcon}</span>
        <span className="text-sm text-text-secondary whitespace-nowrap">{stateLabel}</span>
      </div>

      {/* Audio level visualizer */}
      <div className="flex-1 h-1 bg-border rounded-sm overflow-hidden min-w-[60px]">
        <div
          className="h-full bg-[var(--color-accent,#89b4fa)] rounded-sm transition-[width] duration-[50ms] ease-out"
          style={{ width: `${Math.min(audioLevel * 100, 100)}%` }}
        />
      </div>

      {/* PTT button (only in PTT mode) */}
      {mode === 'ptt' && modelsReady && (
        <button
          className={cn(
            'px-sm py-xs border border-border rounded-md bg-[var(--color-surface-elevated,#2a2a3e)] text-text cursor-pointer text-sm whitespace-nowrap transition-all duration-100 select-none',
            'hover:bg-[var(--color-surface-hover,#353550)]',
            isPttHeld && 'bg-[rgba(243,139,168,0.2)] border-[var(--color-error,#f38ba8)] shadow-[0_0_8px_rgba(243,139,168,0.3)]',
          )}
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
          className="px-sm py-xs border border-border rounded-md bg-[var(--color-surface-elevated,#2a2a3e)] text-text cursor-pointer text-sm whitespace-nowrap hover:bg-[var(--color-surface-hover,#353550)]"
          onClick={() => stopSpeaking?.()}
          title="Stop speaking"
        >
          ⏹️ Stop
        </button>
      )}

      {/* TTS generating indicator */}
      {isTtsGenerating && !isSpeaking && (
        <span className="flex items-center gap-1.5 text-xs text-[var(--color-accent,#89b4fa)] whitespace-nowrap">
          <span className="inline-block w-3 h-3 border-2 border-border border-t-[var(--color-accent,#89b4fa)] rounded-full animate-spin-360 shrink-0" />
          Generating speech…
        </span>
      )}

      {/* Models auto-loading indicator (animated) */}
      {showAutoLoading && (
        <span className="flex items-center gap-1.5 text-xs text-[var(--color-accent,#89b4fa)] whitespace-nowrap">
          <span className="inline-block w-3 h-3 border-2 border-border border-t-[var(--color-accent,#89b4fa)] rounded-full animate-spin-360 shrink-0" />
          Loading models…
        </span>
      )}

      {/* Models not loaded warning (only if NOT currently loading) */}
      {!modelsReady && !showAutoLoading && (
        <span className="text-xs text-[var(--color-warning,#fab387)] whitespace-nowrap">
          Models not loaded — open Voice settings
        </span>
      )}

      {/* Error display */}
      {error && (
        <div className="flex items-center gap-xs text-xs text-[var(--color-error,#f38ba8)] max-w-[200px]">
          <span className="overflow-hidden text-ellipsis whitespace-nowrap">{error}</span>
          <button className="bg-transparent border-none text-[var(--color-error,#f38ba8)] cursor-pointer p-[2px] text-[0.7rem] shrink-0" onClick={() => clearError?.()}>✕</button>
        </div>
      )}

      {/* Close voice mode */}
      <button
        className="bg-transparent border-none text-text-secondary cursor-pointer p-1 text-[0.9rem] shrink-0 rounded-sm hover:text-text hover:bg-[var(--color-surface-hover,#353550)]"
        onClick={() => stop?.()}
        title="Close voice mode"
      >
        ✕
      </button>
    </div>
  );
};
