/**
 * Deep Research Context
 *
 * Provides deep research state and controls to child components.
 * Used to coordinate between the toggle, artifact, and persistence layers.
 *
 * @module contexts/DeepResearchContext
 */

import React, { createContext, useContext, useState, useCallback, useMemo } from 'react';
import type { ResearchState } from '../hooks/useDeepResearch/types';

// =============================================================================
// Types
// =============================================================================

export interface DeepResearchContextValue {
  /** Whether deep research mode is enabled for the next message */
  isEnabled: boolean;
  /** Toggle deep research mode */
  toggleEnabled: () => void;
  /** Set deep research mode explicitly */
  setEnabled: (enabled: boolean) => void;

  /** Current research state (null if not running) */
  researchState: ResearchState | null;
  /** Set the current research state */
  setResearchState: (state: ResearchState | null) => void;

  /** Whether research is actively running */
  isRunning: boolean;
  /** Set running state */
  setIsRunning: (running: boolean) => void;

  /** Stop the current research session */
  stopResearch: () => void;
  /** Set the stop handler */
  setStopHandler: (handler: (() => void) | null) => void;
}

// =============================================================================
// Context
// =============================================================================

const DeepResearchContext = createContext<DeepResearchContextValue | null>(null);

// =============================================================================
// Provider
// =============================================================================

export interface DeepResearchProviderProps {
  children: React.ReactNode;
}

export const DeepResearchProvider: React.FC<DeepResearchProviderProps> = ({
  children,
}) => {
  const [isEnabled, setEnabled] = useState(false);
  const [researchState, setResearchState] = useState<ResearchState | null>(null);
  const [isRunning, setIsRunning] = useState(false);
  const [stopHandler, setStopHandlerInternal] = useState<(() => void) | null>(null);

  const toggleEnabled = useCallback(() => {
    setEnabled((prev) => !prev);
  }, []);

  const stopResearch = useCallback(() => {
    if (stopHandler) {
      stopHandler();
    }
  }, [stopHandler]);

  const setStopHandler = useCallback((handler: (() => void) | null) => {
    setStopHandlerInternal(() => handler);
  }, []);

  const value = useMemo<DeepResearchContextValue>(
    () => ({
      isEnabled,
      toggleEnabled,
      setEnabled,
      researchState,
      setResearchState,
      isRunning,
      setIsRunning,
      stopResearch,
      setStopHandler,
    }),
    [
      isEnabled,
      toggleEnabled,
      researchState,
      isRunning,
      stopResearch,
      setStopHandler,
    ]
  );

  return (
    <DeepResearchContext.Provider value={value}>
      {children}
    </DeepResearchContext.Provider>
  );
};

// =============================================================================
// Hook
// =============================================================================

/**
 * Use the deep research context.
 * Must be used within a DeepResearchProvider.
 */
export function useDeepResearchContext(): DeepResearchContextValue {
  const context = useContext(DeepResearchContext);
  if (!context) {
    throw new Error(
      'useDeepResearchContext must be used within a DeepResearchProvider'
    );
  }
  return context;
}

/**
 * Use the deep research context, returning null if not within a provider.
 * Useful for components that can work with or without deep research.
 */
export function useDeepResearchContextOptional(): DeepResearchContextValue | null {
  return useContext(DeepResearchContext);
}

export default DeepResearchContext;
