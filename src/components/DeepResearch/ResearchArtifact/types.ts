/**
 * Shared types for the ResearchArtifact component family.
 *
 * @module components/DeepResearch/ResearchArtifact/types
 */

import type { ResearchState } from '../../../hooks/useDeepResearch/types';

// Re-export types used by multiple sub-components
export type {
  ResearchState,
  ResearchPhase,
  ResearchQuestion,
  GatheredFact,
  QuestionStatus,
  RoundSummary,
} from '../../../hooks/useDeepResearch/types';

/**
 * Props for the main ResearchArtifact component.
 */
export interface ResearchArtifactProps {
  /** Current research state (live updates) */
  state: ResearchState;
  /** Initial state from database (for re-hydration on reload) */
  initialState?: ResearchState;
  /** Whether research is actively running */
  isRunning: boolean;
  /** Whether to start expanded or collapsed */
  defaultExpanded?: boolean;
  /** Optional className for container */
  className?: string;
  /** Callback to skip a question (mark as blocked) */
  onSkipQuestion?: (questionId: string) => void;
  /** Callback to skip all pending questions */
  onSkipAllPending?: () => void;
  /** Callback to add a user question */
  onAddQuestion?: (question: string) => void;
  /** Callback to generate more questions via AI */
  onGenerateMoreQuestions?: () => void;
  /** Callback to expand a specific question via AI */
  onExpandQuestion?: (questionId: string) => void;
  /** Callback to go deeper via AI */
  onGoDeeper?: () => void;
  /** Callback to force answer generation for a question */
  onForceAnswer?: (questionId: string) => void;
}
