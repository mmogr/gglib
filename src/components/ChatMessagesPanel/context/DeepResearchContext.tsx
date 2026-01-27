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
  /** Skip all pending questions at once */
  skipAllPending?: () => void;
  /** Add a user-specified question to the research plan */
  addQuestion?: (question: string) => void;
  /** Ask AI to generate more research questions */
  generateMoreQuestions?: () => void;
  /** Ask AI to expand a specific question into sub-questions */
  expandQuestion?: (questionId: string) => void;
  /** Ask AI to go deeper based on current findings */
  goDeeper?: () => void;
}

const DeepResearchContext = createContext<DeepResearchContextValue | null>(null);

export interface DeepResearchProviderProps {
  children: React.ReactNode;
  isRunning: boolean;
  skipQuestion?: (questionId: string) => void;
  skipAllPending?: () => void;
  addQuestion?: (question: string) => void;
  generateMoreQuestions?: () => void;
  expandQuestion?: (questionId: string) => void;
  goDeeper?: () => void;
}

/**
 * Provider component for deep research context.
 */
export const DeepResearchProvider: React.FC<DeepResearchProviderProps> = ({
  children,
  isRunning,
  skipQuestion,
  skipAllPending,
  addQuestion,
  generateMoreQuestions,
  expandQuestion,
  goDeeper,
}) => {
  return (
    <DeepResearchContext.Provider value={{
      isRunning,
      skipQuestion,
      skipAllPending,
      addQuestion,
      generateMoreQuestions,
      expandQuestion,
      goDeeper,
    }}>
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
