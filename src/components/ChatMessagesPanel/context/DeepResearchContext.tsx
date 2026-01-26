/**
 * Deep Research Context
 *
 * Provides intervention callbacks for deep research components in the message tree.
 *
 * @module components/ChatMessagesPanel/context/DeepResearchContext
 */

import React, { createContext, useContext } from 'react';

export interface DeepResearchContextValue {
  /** Whether research is currently running */
  isRunning: boolean;
  /** Skip a specific question (mark as blocked) */
  skipQuestion?: (questionId: string) => void;
}

const DeepResearchContext = createContext<DeepResearchContextValue | null>(null);

export interface DeepResearchProviderProps {
  children: React.ReactNode;
  isRunning: boolean;
  skipQuestion?: (questionId: string) => void;
}

/**
 * Provider component for deep research context.
 */
export const DeepResearchProvider: React.FC<DeepResearchProviderProps> = ({
  children,
  isRunning,
  skipQuestion,
}) => {
  return (
    <DeepResearchContext.Provider value={{ isRunning, skipQuestion }}>
      {children}
    </DeepResearchContext.Provider>
  );
};

/**
 * Hook to access deep research context.
 * Returns null if not within a DeepResearchProvider.
 */
export const useDeepResearchContext = (): DeepResearchContextValue | null => {
  return useContext(DeepResearchContext);
};
