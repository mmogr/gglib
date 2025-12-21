import React, { createContext, useContext } from 'react';
import type { ReasoningTimingTracker } from '../../../hooks/useGglibRuntime/reasoningTiming';

/**
 * Context for sharing thinking timing state across message components.
 * Allows live timer updates without recreating component maps every tick.
 */
export interface ThinkingTimingContextValue {
  /** Timing tracker for computing elapsed time and final durations */
  timingTracker: ReasoningTimingTracker | null;
  /** ID of the assistant message currently streaming (for live timer) */
  currentStreamingAssistantMessageId: string | null;
  /** Shared tick counter (increments while streaming to trigger re-renders) */
  tick: number;
}

const ThinkingTimingContext = createContext<ThinkingTimingContextValue | null>(null);

export function ThinkingTimingProvider(props: {
  value: ThinkingTimingContextValue;
  children: React.ReactNode;
}) {
  return <ThinkingTimingContext.Provider value={props.value}>{props.children}</ThinkingTimingContext.Provider>;
}

/**
 * Hook to access thinking timing context.
 * Returns null if not within a ThinkingTimingProvider.
 */
export function useThinkingTiming(): ThinkingTimingContextValue | null {
  return useContext(ThinkingTimingContext);
}
