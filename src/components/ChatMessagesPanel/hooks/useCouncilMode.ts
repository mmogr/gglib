import { useCallback, useEffect, useRef, useState } from 'react';
import type { GglibMessageCustom } from '../../../types/messages';

export interface UseCouncilModeOptions {
  /** Ref the runtime calls when the user submits while council mode is on. */
  councilSubmitRef?: React.MutableRefObject<((text: string) => void) | null>;
  /** Set one-shot metadata on the next user message. */
  setNextMessageMeta?: (meta: Partial<GglibMessageCustom>) => void;
}

export interface UseCouncilModeResult {
  /** Whether the composer is in council (orchestrator) mode. */
  isCouncilMode: boolean;
  /** Flip council mode from the composer toggle. */
  toggleCouncilMode: () => void;
  /** Ref that CouncilThread fills so a run can be started imperatively. */
  councilStartRef: React.MutableRefObject<((goal: string, hitlMode?: string) => void) | null>;
  /** The goal text of the run currently in flight, for the completion callback. */
  pendingGoalRef: React.MutableRefObject<string>;
}

/**
 * Council mode wiring for the chat panel.
 *
 * The toggle state lives here rather than in the composer because it also
 * drives the composer placeholder, the metadata stamped on the next user
 * message, and the submit callback the runtime invokes.
 */
export function useCouncilMode({
  councilSubmitRef,
  setNextMessageMeta,
}: UseCouncilModeOptions): UseCouncilModeResult {
  const [isCouncilMode, setIsCouncilMode] = useState(false);

  const councilStartRef = useRef<((goal: string, hitlMode?: string) => void) | null>(null);
  const pendingGoalRef = useRef<string>('');

  // Register the submit callback so the runtime can start a run on submit.
  useEffect(() => {
    if (councilSubmitRef) {
      councilSubmitRef.current = (text: string) => {
        pendingGoalRef.current = text;
        councilStartRef.current?.(text);
        setIsCouncilMode(false); // Reset toggle after submit
      };
      return () => { councilSubmitRef.current = null; };
    }
  }, [councilSubmitRef]);

  // Sync the mode flag to message metadata before each submission.
  useEffect(() => {
    if (setNextMessageMeta) {
      setNextMessageMeta(isCouncilMode ? { isCouncilMode: true } : {});
    }
  }, [isCouncilMode, setNextMessageMeta]);

  const toggleCouncilMode = useCallback(() => {
    setIsCouncilMode((prev) => !prev);
  }, []);

  return { isCouncilMode, toggleCouncilMode, councilStartRef, pendingGoalRef };
}
