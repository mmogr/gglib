/**
 * Research Artifact Component
 *
 * A "Research Artifact" is a single message bubble that updates in real-time,
 * displaying the deep research progress. Unlike regular chat messages, this
 * artifact shows:
 *
 * 1. Live Activity Header - Current action being performed
 * 2. Monologue Stream - Agent's reasoning (lastReasoning)
 * 3. Research Plan - Questions with completion status
 * 4. Gathered Facts - Extracted knowledge with sources
 * 5. Hypothesis Preview - Current working hypothesis
 * 6. Final Report - Synthesized output when complete
 *
 * Supports collapsed (progress bar only) and expanded (full details) views.
 * Can be re-hydrated from database via initialState prop.
 *
 * @module components/DeepResearch/ResearchArtifact
 */

import React, { useState, useMemo } from 'react';
import {
  Brain,
  ChevronDown,
  ChevronRight,
  Circle,
  CircleCheck,
  CircleX,
  ExternalLink,
  FileSearch,
  Lightbulb,
  Loader2,
  ListTodo,
  Search,
  SkipForward,
  Sparkles,
  AlertTriangle,
  CheckCircle2,
  XCircle,
  History,
  Layers,
  Plus,
  Wand2,
  Maximize2,
  ArrowDownToLine,
  User,
  Bot,
  Download,
  Zap,
} from 'lucide-react';
import { Icon } from '../ui/Icon';
import type {
  ResearchState,
  ResearchPhase,
  ResearchQuestion,
  GatheredFact,
  QuestionStatus,
  RoundSummary,
} from '../../hooks/useDeepResearch/types';
import { useResearchLogExport } from '../../hooks/useResearchLogs';
import { cn } from '../../utils/cn';

// =============================================================================
// Props
// =============================================================================

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

// =============================================================================
// Helper Functions
// =============================================================================

/**
 * Derive the "live activity" text from current state.
 * Shows what the agent is currently doing.
 */
function getLiveActivity(state: ResearchState, isRunning: boolean): string {
  if (!isRunning) {
    if (state.phase === 'complete') return 'Research complete';
    if (state.phase === 'error') return 'Research failed';
    return 'Research paused';
  }

  // Check for LLM generating state (highest priority when thinking)
  if (state.isLLMGenerating) {
    return 'Thinking...';
  }

  // Check for active tool calls with search queries (new verbose tracking)
  if (state.activeToolCalls && state.activeToolCalls.length > 0) {
    const firstTool = state.activeToolCalls[0];
    const elapsed = Math.round((Date.now() - firstTool.startedAt) / 1000);
    
    if (firstTool.searchQuery) {
      const truncated = firstTool.searchQuery.length > 40
        ? firstTool.searchQuery.slice(0, 37) + '...'
        : firstTool.searchQuery;
      return `Searching: "${truncated}" (${elapsed}s)`;
    }
    
    if (state.activeToolCalls.length === 1) {
      return `Running ${firstTool.toolName}... (${elapsed}s)`;
    }
    return `Running ${state.activeToolCalls.length} tools... (${elapsed}s)`;
  }

  // Fallback: Check for pending observations (legacy, for compatibility)
  const pendingTools = state.pendingObservations.filter(o => !o.rawResult);
  if (pendingTools.length > 0) {
    const toolNames = pendingTools.map(o => o.toolName).join(', ');
    if (pendingTools.length === 1) {
      return `Running ${toolNames}...`;
    }
    return `Running ${pendingTools.length} tools (${toolNames})...`;
  }

  // Phase-based activity
  const roundSuffix = state.currentRound > 1 ? ` (Round ${state.currentRound})` : '';
  switch (state.phase) {
    case 'planning':
      if (state.researchPlan.length === 0) {
        return 'Analyzing query and planning research...';
      }
      return 'Refining research plan...';

    case 'gathering': {
      const inProgress = state.researchPlan.filter(q => q.status === 'in-progress');
      if (inProgress.length > 0) {
        return `Researching${roundSuffix}: "${inProgress[0].question.substring(0, 50)}${inProgress[0].question.length > 50 ? '...' : ''}"`;
      }
      const answered = state.researchPlan.filter(q => q.status === 'answered').length;
      const total = state.researchPlan.length;
      return `Gathering information${roundSuffix} (${answered}/${total} questions answered)...`;
    }

    case 'evaluating':
      return `Evaluating research quality${roundSuffix}...`;

    case 'compressing':
      return `Compressing round ${state.currentRound} findings...`;

    case 'synthesizing':
      return 'Synthesizing findings into final report...';

    default:
      return 'Processing...';
  }
}

/**
 * Get phase badge styling and label.
 */
function getPhaseConfig(phase: ResearchPhase): { label: string; className: string } {
  switch (phase) {
    case 'planning':
      return { label: 'Planning', className: 'bg-[rgba(168,85,247,0.2)] text-[#c084fc]' };
    case 'gathering':
      return { label: 'Gathering', className: 'bg-[rgba(59,130,246,0.2)] text-[#60a5fa]' };
    case 'evaluating':
      return { label: 'Evaluating', className: 'bg-[rgba(251,146,60,0.2)] text-[#fb923c]' };
    case 'compressing':
      return { label: 'Compressing', className: 'bg-[rgba(147,51,234,0.2)] text-[#a78bfa]' };
    case 'synthesizing':
      return { label: 'Synthesizing', className: 'bg-[rgba(234,179,8,0.2)] text-[#facc15]' };
    case 'complete':
      return { label: 'Complete', className: 'bg-[rgba(34,197,94,0.2)] text-[#4ade80]' };
    case 'error':
      return { label: 'Error', className: 'bg-[rgba(239,68,68,0.2)] text-[#f87171]' };
  }
}

/**
 * Get question status icon.
 */
function QuestionStatusIcon({ status }: { status: QuestionStatus }) {
  switch (status) {
    case 'pending':
      return <Icon icon={Circle} size={16} className="text-text-muted" />;
    case 'in-progress':
      return <Icon icon={Loader2} size={16} className="text-[#60a5fa] animate-research-pulse" />;
    case 'answered':
      return <Icon icon={CircleCheck} size={16} className="text-[#4ade80]" />;
    case 'blocked':
      return <Icon icon={CircleX} size={16} className="text-[#f87171]" />;
  }
}

/**
 * Calculate progress percentage using diminishing returns for facts.
 * 
 * Progress formula:
 * - Facts contribute 40% via exponential saturation: 0.4 * (1 - e^(-facts/15))
 * - This gives diminishing returns (first 10 facts matter more than next 10)
 * - Maximum 50% from facts alone until questions start getting answered
 * - Questions contribute the remaining 50%: 0.5 * (answered / total)
 * - Cap at 90% until synthesis phase, 100% on complete
 * 
 * This prevents getting "stuck at 10%" when facts are being gathered but
 * questions aren't being answered yet.
 */
function calculateProgress(state: ResearchState): number {
  if (state.phase === 'complete') return 100;
  if (state.phase === 'error') return 0;

  const factCount = state.gatheredFacts.length;
  const totalQuestions = state.researchPlan.length || 1;
  const answeredQuestions = state.researchPlan.filter(q => q.status === 'answered').length;

  // Diminishing returns for fact gathering: 0.4 * (1 - e^(-facts/15))
  // At 15 facts: ~25%, at 30 facts: ~35%, asymptotes at 40%
  const factProgress = 0.4 * (1 - Math.exp(-factCount / 15));

  // Question progress: 0.5 * (answered / total)
  const questionProgress = 0.5 * (answeredQuestions / totalQuestions);

  // Combined progress
  let progress = 0;

  if (state.phase === 'planning') {
    // Planning phase: 0-10%
    if (state.researchPlan.length > 0) {
      progress = 0.1; // Plan exists
    } else {
      progress = state.currentStep / Math.max(1, state.maxSteps) * 0.1;
    }
  } else if (state.phase === 'gathering') {
    // Gathering phase: 10-80%
    // Start with 10% for having a plan
    progress = 0.1;
    
    // Add fact progress (capped at 50% until questions are answered)
    const cappedFactProgress = Math.min(factProgress, 0.4);
    progress += cappedFactProgress;
    
    // Add question progress
    progress += questionProgress;
    
    // Cap at 80% in gathering phase
    progress = Math.min(progress, 0.8);
  } else if (state.phase === 'synthesizing') {
    // Synthesis phase: 80-100%
    progress = 0.9; // Almost done
  }

  return Math.min(100, Math.round(progress * 100));
}

// =============================================================================
// Sub-Components
// =============================================================================

/**
 * Thinking/Reasoning block showing lastReasoning.
 */
const ThinkingBlock: React.FC<{ reasoning: string | null }> = ({ reasoning }) => {
  if (!reasoning) return null;

  return (
    <div className="px-3.5 py-3 bg-background border-b border-border">
      <div className="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-[0.5px] text-text-muted mb-2">
        <Icon icon={Brain} size={12} />
        <span>Thinking</span>
      </div>
      <div className="text-xs text-text-secondary leading-normal italic max-h-[100px] overflow-y-auto whitespace-pre-wrap">{reasoning}</div>
    </div>
  );
};

/**
 * Research plan section showing questions and their status.
 * Enhanced with user controls for adding questions and AI-directed actions.
 */
const ResearchPlanSection: React.FC<{
  questions: ResearchQuestion[];
  facts: GatheredFact[];
  onSkipQuestion?: (questionId: string) => void;
  onSkipAllPending?: () => void;
  onAddQuestion?: (question: string) => void;
  onGenerateMoreQuestions?: () => void;
  onExpandQuestion?: (questionId: string) => void;
  onGoDeeper?: () => void;
  onForceAnswer?: (questionId: string) => void;
  isRunning: boolean;
  isCompleted?: boolean;
}> = ({
  questions,
  facts,
  onSkipQuestion,
  onSkipAllPending,
  onAddQuestion,
  onGenerateMoreQuestions,
  onExpandQuestion,
  onGoDeeper,
  onForceAnswer,
  isRunning,
  isCompleted = false,
}) => {
  // Track which questions have skip pending (optimistic UI)
  const [pendingSkips, setPendingSkips] = useState<Set<string>>(new Set());
  // Track which questions have expand pending
  const [pendingExpands, setPendingExpands] = useState<Set<string>>(new Set());
  // Track if generating more questions is pending
  const [isGenerating, setIsGenerating] = useState(false);
  // Track if going deeper is pending
  const [isGoingDeeper, setIsGoingDeeper] = useState(false);
  // Track which questions have force-answer pending
  const [pendingForceAnswers, setPendingForceAnswers] = useState<Set<string>>(new Set());
  // State for add question input
  const [newQuestionText, setNewQuestionText] = useState('');
  const [showAddInput, setShowAddInput] = useState(false);
  
  const handleSkip = (questionId: string) => {
    if (onSkipQuestion) {
      // Optimistic UI - disable button immediately
      setPendingSkips(prev => new Set(prev).add(questionId));
      onSkipQuestion(questionId);
    }
  };
  
  const handleSkipAll = () => {
    if (onSkipAllPending) {
      onSkipAllPending();
    }
  };
  
  const handleAddQuestion = () => {
    if (onAddQuestion && newQuestionText.trim()) {
      onAddQuestion(newQuestionText.trim());
      setNewQuestionText('');
      setShowAddInput(false);
    }
  };
  
  const handleGenerateMore = () => {
    if (onGenerateMoreQuestions) {
      setIsGenerating(true);
      onGenerateMoreQuestions();
      // Reset after a short delay (the actual state will update from the hook)
      setTimeout(() => setIsGenerating(false), 3000);
    }
  };
  
  const handleExpand = (questionId: string) => {
    if (onExpandQuestion) {
      setPendingExpands(prev => new Set(prev).add(questionId));
      onExpandQuestion(questionId);
      // Reset after a short delay
      setTimeout(() => {
        setPendingExpands(prev => {
          const next = new Set(prev);
          next.delete(questionId);
          return next;
        });
      }, 3000);
    }
  };
  
  const handleGoDeeper = () => {
    if (onGoDeeper) {
      setIsGoingDeeper(true);
      onGoDeeper();
      // Reset after a short delay
      setTimeout(() => setIsGoingDeeper(false), 3000);
    }
  };

  const handleForceAnswer = (questionId: string) => {
    if (onForceAnswer) {
      setPendingForceAnswers(prev => new Set(prev).add(questionId));
      onForceAnswer(questionId);
      // Reset after a longer delay (LLM generation takes time)
      setTimeout(() => {
        setPendingForceAnswers(prev => {
          const next = new Set(prev);
          next.delete(questionId);
          return next;
        });
      }, 15000);
    }
  };
  
  // Count pending questions
  const pendingCount = questions.filter(
    q => q.status === 'pending' || q.status === 'in-progress'
  ).length;
  
  // Check if actions are available (only during gathering phase when running)
  const actionsAvailable = !isCompleted && isRunning;
  
  if (questions.length === 0) {
    return (
      <div className="px-3.5 py-3 border-b border-border last:border-b-0">
        <div className="flex items-center justify-between mb-2.5">
          <span className="flex items-center gap-1.5 text-xs font-semibold text-text uppercase tracking-[0.5px]">
            <Icon icon={ListTodo} size={14} />
            Research Plan
          </span>
        </div>
        <div className="text-center p-4 text-text-muted text-xs italic">No research questions yet...</div>
      </div>
    );
  }

  // Sort: in-progress first, then pending by priority, then answered, blocked last
  const sortedQuestions = [...questions].sort((a, b) => {
    // Define status priority (lower = higher priority)
    const statusOrder: Record<QuestionStatus, number> = {
      'in-progress': 0,
      'pending': 1,
      'answered': 2,
      'blocked': 3,
    };
    
    // First sort by status
    const statusDiff = statusOrder[a.status] - statusOrder[b.status];
    if (statusDiff !== 0) return statusDiff;
    
    // Within same status, sort by priority
    return a.priority - b.priority;
  });

  return (
    <div className="px-3.5 py-3 border-b border-border last:border-b-0">
      <div className="flex items-center justify-between mb-2.5">
        <span className="flex items-center gap-1.5 text-xs font-semibold text-text uppercase tracking-[0.5px]">
          <Icon icon={ListTodo} size={14} />
          Research Plan
        </span>
        <span className="text-[11px] text-text-muted font-normal">
          {questions.filter(q => q.status === 'answered').length}/{questions.length} answered
        </span>
      </div>
      
      {/* Action buttons row - only show when research is running */}
      {actionsAvailable && (
        <div className="flex flex-wrap gap-1.5 py-2 mb-2 border-b border-border">
          {/* Add question button/input */}
          {showAddInput ? (
            <div className="flex items-center gap-1 flex-1 min-w-[200px]">
              <input
                type="text"
                value={newQuestionText}
                onChange={(e) => setNewQuestionText(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && newQuestionText.trim()) {
                    handleAddQuestion();
                  } else if (e.key === 'Escape') {
                    setShowAddInput(false);
                    setNewQuestionText('');
                  }
                }}
                placeholder="Type your question..."
                className="flex-1 px-2 py-1 text-xs text-text bg-background-secondary border border-[#60a5fa] rounded outline-none placeholder:text-text-muted"
                autoFocus
              />
              <button
                className="flex items-center justify-center w-6 h-6 p-0 text-[#60a5fa] bg-[rgba(96,165,250,0.1)] border border-[#60a5fa] rounded cursor-pointer transition-all duration-150 ease-out hover:enabled:bg-[rgba(96,165,250,0.2)] disabled:opacity-40 disabled:cursor-default"
                onClick={handleAddQuestion}
                disabled={!newQuestionText.trim()}
                title="Add question"
                type="button"
              >
                <Icon icon={Plus} size={14} />
              </button>
              <button
                className="flex items-center justify-center w-6 h-6 p-0 text-base text-text-muted bg-transparent border border-border rounded cursor-pointer transition-all duration-150 ease-out hover:text-text hover:bg-background-tertiary"
                onClick={() => {
                  setShowAddInput(false);
                  setNewQuestionText('');
                }}
                title="Cancel"
                type="button"
              >
                ×
              </button>
            </div>
          ) : (
            <button
              className="flex items-center gap-1 px-2 py-1 text-[11px] font-medium text-text-secondary bg-background-tertiary border border-border rounded cursor-pointer transition-all duration-150 ease-out hover:text-text hover:bg-background-hover hover:border-[#4a4a4a] disabled:opacity-50 disabled:cursor-default disabled:hover:bg-background-tertiary disabled:hover:border-border"
              onClick={() => setShowAddInput(true)}
              title="Add your own research question"
              type="button"
            >
              <Icon icon={Plus} size={12} />
              Add Question
            </button>
          )}
          
          {/* AI action buttons */}
          <button
            className="flex items-center gap-1 px-2 py-1 text-[11px] font-medium text-text-secondary bg-background-tertiary border border-border rounded cursor-pointer transition-all duration-150 ease-out hover:text-text hover:bg-background-hover hover:border-[#4a4a4a] disabled:opacity-50 disabled:cursor-default disabled:hover:bg-background-tertiary disabled:hover:border-border"
            onClick={handleGenerateMore}
            disabled={isGenerating}
            title="Ask AI to generate more research questions"
            type="button"
          >
            {isGenerating ? (
              <Icon icon={Loader2} size={12} className="animate-spin" />
            ) : (
              <Icon icon={Wand2} size={12} />
            )}
            More Questions
          </button>
          
          <button
            className="flex items-center gap-1 px-2 py-1 text-[11px] font-medium text-text-secondary bg-background-tertiary border border-border rounded cursor-pointer transition-all duration-150 ease-out hover:text-text hover:bg-background-hover hover:border-[#4a4a4a] disabled:opacity-50 disabled:cursor-default disabled:hover:bg-background-tertiary disabled:hover:border-border"
            onClick={handleGoDeeper}
            disabled={isGoingDeeper}
            title="Ask AI to explore deeper based on current findings"
            type="button"
          >
            {isGoingDeeper ? (
              <Icon icon={Loader2} size={12} className="animate-spin" />
            ) : (
              <Icon icon={ArrowDownToLine} size={12} />
            )}
            Go Deeper
          </button>
          
          {/* Skip all pending - only show if there are multiple pending questions */}
          {pendingCount > 1 && onSkipAllPending && (
            <button
              className="flex items-center gap-1 px-2 py-1 text-[11px] font-medium text-[#f87171] bg-background-tertiary border border-border rounded cursor-pointer transition-all duration-150 ease-out hover:bg-[rgba(248,113,113,0.1)] hover:border-[rgba(248,113,113,0.3)] disabled:opacity-50 disabled:cursor-default"
              onClick={handleSkipAll}
              title="Skip all remaining questions"
              type="button"
            >
              <Icon icon={SkipForward} size={12} />
              Skip All ({pendingCount})
            </button>
          )}
        </div>
      )}
      
      <div className="flex flex-col gap-2">
        {sortedQuestions.map(question => {
          const canSkip = actionsAvailable && 
            onSkipQuestion && 
            (question.status === 'in-progress' || question.status === 'pending') &&
            !pendingSkips.has(question.id);
          
          const canExpand = actionsAvailable &&
            onExpandQuestion &&
            question.status === 'pending' &&
            !pendingExpands.has(question.id);
          
          // Force-answer: available for in-progress questions with facts
          const relevantFactCount = facts.filter(f => 
            f.relevantQuestionIds.includes(question.id)
          ).length;
          const canForceAnswer = actionsAvailable &&
            onForceAnswer &&
            question.status === 'in-progress' &&
            !pendingForceAnswers.has(question.id) &&
            facts.length > 0; // Need at least some facts
          
          const isBlocked = question.status === 'blocked';
          const showSkipButton = canSkip || (isCompleted && (question.status === 'in-progress' || question.status === 'pending'));
          
          // Determine question source indicator
          const sourceIcon = question.source === 'user-added' ? User :
                           question.source === 'ai-expanded' ? Maximize2 :
                           question.source === 'ai-generated' ? Bot :
                           null;
          
          return (
            <div 
              key={question.id} 
              className={cn(
                'group flex items-start gap-2.5 px-2.5 py-2 bg-background-tertiary rounded-md',
                isBlocked && 'opacity-50 bg-transparent',
              )}
            >
              <div className="shrink-0 w-[18px] h-[18px] flex items-center justify-center mt-px">
                <QuestionStatusIcon status={question.status} />
              </div>
              <div className="flex-1 min-w-0">
                <div className={cn(
                  'text-[13px] text-text leading-[1.4]',
                  isBlocked && 'line-through text-text-muted',
                )}>
                  {sourceIcon && (
                    <span className="inline-flex items-center justify-center mr-1 text-text-muted align-middle" title={`Source: ${question.source}`}>
                      <Icon icon={sourceIcon} size={10} />
                    </span>
                  )}
                  {question.question}
                </div>
                {question.answerSummary && (
                  <div className="mt-1 text-xs text-text-secondary leading-[1.4]">{question.answerSummary}</div>
                )}
              </div>
              <div className="flex items-center gap-1 shrink-0">
                {/* Expand button */}
                {canExpand && (
                  <button
                    className="shrink-0 flex items-center justify-center w-6 h-6 border-none rounded bg-transparent text-text-muted cursor-pointer opacity-0 transition-all duration-150 ease-out group-hover:opacity-100 hover:bg-[rgba(96,165,250,0.15)] hover:text-[#60a5fa] active:bg-[rgba(96,165,250,0.25)]"
                    onClick={() => handleExpand(question.id)}
                    title="Ask AI to break this into sub-questions"
                    type="button"
                  >
                    {pendingExpands.has(question.id) ? (
                      <Icon icon={Loader2} size={12} className="animate-spin" />
                    ) : (
                      <Icon icon={Maximize2} size={12} />
                    )}
                  </button>
                )}
                {/* Force-answer / Synthesize button - for in-progress questions */}
                {canForceAnswer && (
                  <button
                    className="shrink-0 flex items-center justify-center w-6 h-6 border-none rounded bg-transparent text-text-muted cursor-pointer opacity-0 transition-all duration-150 ease-out group-hover:opacity-100 hover:bg-[rgba(251,191,36,0.15)] hover:text-[#fbbf24] active:bg-[rgba(251,191,36,0.25)]"
                    onClick={() => handleForceAnswer(question.id)}
                    title={`Generate answer now with ${relevantFactCount > 0 ? relevantFactCount : facts.length} facts`}
                    type="button"
                  >
                    {pendingForceAnswers.has(question.id) ? (
                      <Icon icon={Loader2} size={12} className="animate-spin" />
                    ) : (
                      <Icon icon={Zap} size={12} />
                    )}
                  </button>
                )}
                {/* Skip button */}
                {showSkipButton && (
                  <button
                    className="shrink-0 flex items-center justify-center w-6 h-6 border-none rounded bg-transparent text-text-muted cursor-pointer opacity-0 transition-all duration-150 ease-out group-hover:opacity-100 hover:bg-[rgba(251,146,60,0.15)] hover:text-[#fb923c] active:bg-[rgba(251,146,60,0.25)] disabled:opacity-50 disabled:cursor-not-allowed disabled:bg-transparent disabled:hover:bg-transparent disabled:hover:text-text-muted"
                    onClick={() => handleSkip(question.id)}
                    title={isCompleted ? "Question was skipped" : "Skip this question"}
                    type="button"
                    disabled={isCompleted}
                  >
                    <Icon icon={SkipForward} size={12} />
                  </button>
                )}
                {pendingSkips.has(question.id) && (
                  <div className="flex items-center justify-center w-6 h-6 text-[#fb923c]">
                    <Icon icon={Loader2} size={12} className="animate-spin" />
                  </div>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};

/**
 * Gathered facts section.
 */
const GatheredFactsSection: React.FC<{ facts: GatheredFact[] }> = ({ facts }) => {
  const [showAll, setShowAll] = useState(false);
  const displayLimit = 5;

  if (facts.length === 0) {
    return (
      <div className="px-3.5 py-3 border-b border-border last:border-b-0">
        <div className="flex items-center justify-between mb-2.5">
          <span className="flex items-center gap-1.5 text-xs font-semibold text-text uppercase tracking-[0.5px]">
            <Icon icon={FileSearch} size={14} />
            Gathered Facts
          </span>
        </div>
        <div className="text-center p-4 text-text-muted text-xs italic">No facts gathered yet...</div>
      </div>
    );
  }

  // Sort by most recent first
  const sortedFacts = [...facts].sort((a, b) => b.gatheredAtStep - a.gatheredAtStep);
  const displayedFacts = showAll ? sortedFacts : sortedFacts.slice(0, displayLimit);

  return (
    <div className="px-3.5 py-3 border-b border-border last:border-b-0">
      <div className="flex items-center justify-between mb-2.5">
        <span className="flex items-center gap-1.5 text-xs font-semibold text-text uppercase tracking-[0.5px]">
          <Icon icon={FileSearch} size={14} />
          Gathered Facts
        </span>
        <span className="text-[11px] text-text-muted font-normal">{facts.length} facts</span>
      </div>
      <div className="flex flex-col gap-2">
        {displayedFacts.map(fact => (
          <div
            key={fact.id}
            className="px-2.5 py-2 bg-background-tertiary rounded-md border-l-[3px] border-l-transparent data-[confidence=high]:border-l-[#4ade80] data-[confidence=medium]:border-l-[#facc15] data-[confidence=low]:border-l-[#f87171]"
            data-confidence={fact.confidence}
          >
            <div className="text-[13px] text-text leading-[1.4]">{fact.claim}</div>
            <div className="flex items-center gap-2 mt-1.5 text-[11px] text-text-muted">
              <a
                href={fact.sourceUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="flex items-center gap-1 text-[#60a5fa] no-underline max-w-[200px] overflow-hidden text-ellipsis whitespace-nowrap hover:underline"
                title={fact.sourceUrl}
              >
                <Icon icon={ExternalLink} size={10} />
                {fact.sourceTitle || new URL(fact.sourceUrl).hostname}
              </a>
              <span className="flex items-center gap-1">
                {fact.confidence === 'high' && <Icon icon={CheckCircle2} size={10} />}
                {fact.confidence === 'medium' && <Icon icon={AlertTriangle} size={10} />}
                {fact.confidence === 'low' && <Icon icon={XCircle} size={10} />}
                {fact.confidence}
              </span>
            </div>
          </div>
        ))}
      </div>
      {facts.length > displayLimit && (
        <button
          className="block w-full mt-2 px-3 py-1.5 bg-background-tertiary border border-border rounded text-text-secondary text-xs cursor-pointer transition-all duration-200 ease-out hover:bg-background-hover hover:text-text"
          onClick={() => setShowAll(!showAll)}
        >
          {showAll ? 'Show less' : `Show ${facts.length - displayLimit} more`}
        </button>
      )}
    </div>
  );
};

/**
 * Hypothesis preview block.
 */
const HypothesisBlock: React.FC<{ hypothesis: string | null }> = ({ hypothesis }) => {
  if (!hypothesis) return null;

  return (
    <div className="px-3.5 py-3 border-b border-border last:border-b-0">
      <div className="px-3.5 py-3 bg-background rounded-md">
        <div className="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-[0.5px] text-text-muted mb-2">
          <Icon icon={Lightbulb} size={12} />
          <span>Working Hypothesis</span>
        </div>
        <div className="text-[13px] text-text leading-normal">{hypothesis}</div>
      </div>
    </div>
  );
};

/**
 * Knowledge gaps section.
 */
const KnowledgeGapsSection: React.FC<{ gaps: string[] }> = ({ gaps }) => {
  if (gaps.length === 0) return null;

  return (
    <div className="px-3.5 py-3 border-b border-border last:border-b-0">
      <div className="flex items-center justify-between mb-2.5">
        <span className="flex items-center gap-1.5 text-xs font-semibold text-text uppercase tracking-[0.5px]">
          <Icon icon={Search} size={14} />
          Knowledge Gaps
        </span>
      </div>
      <div className="flex flex-wrap gap-1.5">
        {gaps.map((gap, idx) => (
          <span key={idx} className="inline-flex items-center gap-1 px-2 py-1 bg-[rgba(234,179,8,0.1)] border border-[rgba(234,179,8,0.3)] rounded text-[11px] text-[#facc15]">
            <Icon icon={AlertTriangle} size={10} />
            {gap}
          </span>
        ))}
      </div>
    </div>
  );
};

/**
 * Previous rounds section - shows compressed summaries from prior research rounds.
 * Only visible when there are completed rounds (currentRound > 1).
 */
const PreviousRoundsSection: React.FC<{
  roundSummaries: RoundSummary[];
}> = ({ roundSummaries }) => {
  const [expanded, setExpanded] = useState(false);

  if (roundSummaries.length === 0) return null;

  return (
    <div className="px-3.5 py-3 border-b border-border last:border-b-0">
      <div
        className="flex items-center justify-between mb-2.5"
        onClick={() => setExpanded(!expanded)}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            setExpanded(!expanded);
          }
        }}
        style={{ cursor: 'pointer' }}
      >
        <span className="flex items-center gap-1.5 text-xs font-semibold text-text uppercase tracking-[0.5px]">
          <Icon icon={History} size={14} />
          Previous Rounds
        </span>
        <span className="text-[11px] text-text-muted font-normal">
          {roundSummaries.length} round{roundSummaries.length !== 1 ? 's' : ''}
          <Icon
            icon={expanded ? ChevronDown : ChevronRight}
            size={14}
            style={{ marginLeft: 4 }}
          />
        </span>
      </div>
      {expanded && (
        <div className="flex flex-col gap-2.5 px-3 py-2.5">
          {roundSummaries.map((round) => (
            <div key={round.round} className="p-3 bg-background-tertiary border border-border rounded-lg border-l-[3px] border-l-[#a78bfa]">
              <div className="flex items-center justify-between mb-2">
                <span className="flex items-center gap-1.5 text-xs font-semibold text-[#a78bfa]">
                  <Icon icon={Layers} size={12} />
                  Round {round.round}
                  {round.perspective && (
                    <span className="font-medium text-[#c4b5fd] italic ml-1">
                      ({round.perspective})
                    </span>
                  )}
                </span>
                <span className="text-[11px] text-text-muted">
                  {round.factCountAtEnd} facts · {round.questionsAnsweredThisRound.length} questions
                </span>
              </div>
              <div className="text-xs text-text-secondary leading-normal mb-2">
                {round.summary}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};

/**
 * Final report section (when complete).
 * Renders citations [1], [2], etc. as hoverable cards showing fact details.
 */
const FinalReportSection: React.FC<{
  report: string;
  facts: GatheredFact[];
  citations: ResearchState['citations'];
}> = ({ report, facts, citations }) => {
  // Build a map of citation number -> fact for quick lookup
  const citationToFact = useMemo(() => {
    const map = new Map<number, GatheredFact>();
    
    // If we have explicit citations array, use that
    if (citations && citations.length > 0) {
      citations.forEach((cit, idx) => {
        const fact = facts.find(f => f.id === cit.factId);
        if (fact) {
          map.set(idx + 1, fact);
        }
      });
    } else {
      // Fallback: assume facts are in citation order
      facts.forEach((fact, idx) => {
        map.set(idx + 1, fact);
      });
    }
    
    return map;
  }, [facts, citations]);

  // Parse report and replace [N] with interactive citations
  const renderedReport = useMemo(() => {
    // Match citation patterns like [1], [2], [12], etc.
    const citationRegex = /\[(\d+)\]/g;
    const parts: React.ReactNode[] = [];
    let lastIndex = 0;
    let match: RegExpExecArray | null;
    let keyIdx = 0;

    while ((match = citationRegex.exec(report)) !== null) {
      // Add text before this citation
      if (match.index > lastIndex) {
        parts.push(report.slice(lastIndex, match.index));
      }

      const citNum = parseInt(match[1], 10);
      const fact = citationToFact.get(citNum);

      if (fact) {
        // Render interactive citation with hover card
        parts.push(
          <CitationRef key={`cit-${keyIdx++}`} number={citNum} fact={fact} />
        );
      } else {
        // No fact found, render as plain text
        parts.push(match[0]);
      }

      lastIndex = match.index + match[0].length;
    }

    // Add remaining text
    if (lastIndex < report.length) {
      parts.push(report.slice(lastIndex));
    }

    return parts;
  }, [report, citationToFact]);

  return (
    <div className="p-4">
      <div className="text-sm text-text leading-relaxed whitespace-pre-wrap">{renderedReport}</div>
    </div>
  );
};

/**
 * Individual citation reference with hover card.
 */
const CitationRef: React.FC<{ number: number; fact: GatheredFact }> = ({
  number,
  fact,
}) => {
  // Truncate claim for display
  const displayClaim =
    fact.claim.length > 200 ? `${fact.claim.slice(0, 200)}...` : fact.claim;

  // Get hostname for display
  const hostname = useMemo(() => {
    try {
      return new URL(fact.sourceUrl).hostname.replace(/^www\./, '');
    } catch {
      return fact.sourceUrl;
    }
  }, [fact.sourceUrl]);

  return (
    <span className="group/cite relative inline cursor-pointer text-[#60a5fa] font-semibold text-[0.85em] align-super px-0.5 rounded-sm transition-all duration-150 ease-out hover:bg-[rgba(96,165,250,0.15)] hover:text-[#93c5fd]" tabIndex={0} role="button">
      [{number}]
      <span className="absolute bottom-[calc(100%+8px)] left-1/2 -translate-x-1/2 w-[320px] max-w-[90vw] bg-background-tertiary border border-[#444] rounded-lg p-3 shadow-[0_4px_20px_rgba(0,0,0,0.4)] z-[1000] opacity-0 invisible transition-[opacity,visibility] duration-150 ease-out pointer-events-none group-hover/cite:opacity-100 group-hover/cite:visible group-hover/cite:pointer-events-auto group-focus/cite:opacity-100 group-focus/cite:visible group-focus/cite:pointer-events-auto after:content-[''] after:absolute after:top-full after:left-1/2 after:-translate-x-1/2 after:border-[6px] after:border-transparent after:border-t-border">
        <div className="flex items-start gap-2 mb-2">
          <span className="flex items-center justify-center w-5 h-5 rounded bg-[rgba(96,165,250,0.2)] text-[#60a5fa] text-[11px] font-bold shrink-0">{number}</span>
          <span className="font-semibold text-text text-[13px] leading-[1.3] flex-1 min-w-0 overflow-hidden text-ellipsis line-clamp-2">{fact.sourceTitle}</span>
        </div>
        <div className="text-xs text-text-secondary leading-normal mb-2 p-2 bg-background-secondary rounded border-l-2 border-l-[rgba(96,165,250,0.5)]">{displayClaim}</div>
        <div className="flex items-center justify-between gap-2 text-[11px]">
          <span
            className="flex items-center gap-1 text-text-muted data-[confidence=high]:text-[#4ade80] data-[confidence=medium]:text-[#fbbf24] data-[confidence=low]:text-[#f87171]"
            data-confidence={fact.confidence}
          >
            <CheckCircle2 size={12} />
            {fact.confidence} confidence
          </span>
          <a
            href={fact.sourceUrl}
            target="_blank"
            rel="noopener noreferrer"
            className="flex items-center gap-1 text-[#60a5fa] no-underline text-[11px] max-w-[150px] overflow-hidden text-ellipsis whitespace-nowrap hover:underline"
            onClick={(e) => e.stopPropagation()}
          >
            <ExternalLink size={10} />
            {hostname}
          </a>
        </div>
      </span>
    </span>
  );
};

// =============================================================================
// Main Component
// =============================================================================

export const ResearchArtifact: React.FC<ResearchArtifactProps> = ({
  state,
  initialState,
  isRunning,
  defaultExpanded = true,
  className,
  onSkipQuestion,
  onSkipAllPending,
  onAddQuestion,
  onGenerateMoreQuestions,
  onExpandQuestion,
  onGoDeeper,
  onForceAnswer,
}) => {
  const [expanded, setExpanded] = useState(defaultExpanded);
  const [detailsExpanded, setDetailsExpanded] = useState(false);
  
  // Log export functionality
  const { downloadAsJSON } = useResearchLogExport(state.messageId);

  // Use initialState for re-hydration if state is empty
  const effectiveState = useMemo(() => {
    // If we have a fresh state with no progress but initialState exists, use that
    if (initialState && state.currentStep === 0 && state.researchPlan.length === 0) {
      return initialState;
    }
    return state;
  }, [state, initialState]);

  const phaseConfig = getPhaseConfig(effectiveState.phase);
  const liveActivity = getLiveActivity(effectiveState, isRunning);
  const progress = calculateProgress(effectiveState);

  return (
    <div
      className={cn(
        'bg-background-secondary border border-border rounded-xl my-3 overflow-hidden text-[13px] max-w-full',
        'data-[running=true]:border-[rgba(59,130,246,0.4)] data-[running=true]:shadow-[0_0_0_1px_rgba(59,130,246,0.1)]',
        className,
      )}
      data-running={isRunning}
    >
      {/* Header (always visible) */}
      <div
        className="flex items-center gap-2.5 px-3.5 py-3 bg-background-tertiary border-b border-border cursor-pointer select-none hover:bg-background-hover"
        onClick={() => setExpanded(!expanded)}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            setExpanded(!expanded);
          }
        }}
        aria-expanded={expanded}
      >
        <div className="flex items-center justify-center w-8 h-8 rounded-lg bg-gradient-to-br from-[#6366f1] to-[#8b5cf6] text-white shrink-0">
          <Icon icon={Sparkles} size={18} />
        </div>

        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 font-semibold text-text text-sm">
            <span>Deep Research</span>
            <span className={cn(
              'inline-flex items-center gap-1 px-2 py-0.5 rounded-xl text-[10px] font-semibold uppercase tracking-[0.5px]',
              phaseConfig.className,
            )}>
              {phaseConfig.label}
            </span>
            {effectiveState.maxRounds > 1 && (
              <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-xl text-[10px] font-semibold bg-[rgba(100,116,139,0.2)] text-[#94a3b8] ml-1.5">
                Round {effectiveState.currentRound}/{effectiveState.maxRounds}
              </span>
            )}
            {effectiveState.currentPerspective && (
              <span className="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-xl text-[10px] font-semibold bg-[rgba(147,51,234,0.25)] text-[#c4b5fd] ml-1.5 border border-[rgba(147,51,234,0.4)] max-w-[200px] overflow-hidden text-ellipsis whitespace-nowrap">
                🎭 {effectiveState.currentPerspective}
              </span>
            )}
          </div>

          <div className="flex items-center gap-1.5 mt-1 text-xs text-text-secondary">
            {isRunning && (
              <Icon icon={Loader2} size={12} className="animate-spin text-[#60a5fa]" />
            )}
            <span>{liveActivity}</span>
          </div>
        </div>

        {/* Download logs button - always visible in header */}
        <button
          className="flex items-center justify-center w-7 h-7 rounded-md border border-border bg-background-secondary text-text-muted cursor-pointer transition-all duration-150 ease-out shrink-0 hover:text-text hover:bg-background-hover hover:border-border-hover active:scale-95"
          onClick={(e) => {
            e.stopPropagation();
            downloadAsJSON();
          }}
          title="Download research logs (JSON)"
        >
          <Icon icon={Download} size={14} />
        </button>

        <div className="flex items-center justify-center w-6 h-6 rounded text-text-muted shrink-0 transition-transform duration-200 ease-out data-[expanded=true]:rotate-180" data-expanded={expanded}>
          <Icon icon={ChevronDown} size={18} />
        </div>
      </div>

      {/* Progress bar (always visible when not complete) */}
      {effectiveState.phase !== 'complete' && effectiveState.phase !== 'error' && (
        <div className="px-3.5 pb-3">
          <div className="h-1 bg-background rounded-sm overflow-hidden">
            <div
              className="h-full bg-gradient-to-r from-[#6366f1] to-[#8b5cf6] rounded-sm transition-[width] duration-300 ease-out"
              style={{ width: `${progress}%` }}
            />
          </div>
          <div className="flex justify-between mt-1.5 text-[11px] text-text-muted">
            <span>Step {effectiveState.currentStep}/{effectiveState.maxSteps}</span>
            <span>{progress}%</span>
          </div>
        </div>
      )}

      {/* Activity log (trailing events for velocity visibility) */}
      {isRunning && effectiveState.activityLog && effectiveState.activityLog.length > 0 && (
        <div className="px-3.5 py-2 bg-background-tertiary border-t border-border flex flex-col gap-1">
          {effectiveState.activityLog.map((entry, idx) => {
            const isSkipped = entry.startsWith('Search skipped:') || entry.includes('(duplicate)');
            return (
              <div
                key={idx}
                className={cn(
                  "text-[11px] text-text-secondary font-mono whitespace-nowrap overflow-hidden text-ellipsis transition-opacity duration-200 ease-out before:content-['›'] before:mr-1.5 before:text-text-muted",
                  isSkipped && "text-text-muted italic before:content-['⊘'] before:text-[rgba(251,146,60,0.6)]",
                )}
                style={{ opacity: 0.5 + (idx / effectiveState.activityLog.length) * 0.5 }}
              >
                {entry}
              </div>
            );
          })}
        </div>
      )}

      {/* Collapsed summary stats */}
      {!expanded && (
        <div className="px-3.5 py-2 pb-3">
          <div className="flex gap-4 text-[11px] text-text-muted">
            <span className="flex items-center gap-1 [&>strong]:text-text-secondary">
              <strong>{effectiveState.researchPlan.length}</strong> questions
            </span>
            <span className="flex items-center gap-1 [&>strong]:text-text-secondary">
              <strong>{effectiveState.gatheredFacts.length}</strong> facts
            </span>
            {effectiveState.knowledgeGaps.length > 0 && (
              <span className="flex items-center gap-1 [&>strong]:text-text-secondary">
                <strong>{effectiveState.knowledgeGaps.length}</strong> gaps
              </span>
            )}
          </div>
        </div>
      )}

      {/* Expanded content */}
      {expanded && (
        <div className="border-t border-border">
          {/* Thinking/Reasoning block */}
          <ThinkingBlock reasoning={effectiveState.lastReasoning} />

          {/* Show final report if complete */}
          {effectiveState.phase === 'complete' && effectiveState.finalReport ? (
            <>
              <FinalReportSection
                report={effectiveState.finalReport}
                facts={effectiveState.gatheredFacts}
                citations={effectiveState.citations}
              />

              {/* Collapsible Research Details Section */}
              <div className="border-t border-border bg-background-tertiary">
                <div
                  className="flex items-center justify-between px-3.5 py-2.5 cursor-pointer select-none transition-colors duration-150 ease-out hover:bg-background-hover"
                  onClick={() => setDetailsExpanded(!detailsExpanded)}
                >
                  <div className="flex items-center gap-1.5 text-xs font-semibold text-text-secondary uppercase tracking-[0.5px]">
                    <Icon icon={FileSearch} size={14} />
                    Research Details
                  </div>
                  <div className="flex items-center gap-2">
                    <button
                      className="flex items-center gap-1 px-2 py-1 text-[11px] font-medium text-text-secondary bg-background-secondary border border-border rounded cursor-pointer transition-all duration-150 ease-out hover:text-text hover:bg-background-hover hover:border-border-hover active:scale-[0.97]"
                      onClick={(e) => {
                        e.stopPropagation();
                        downloadAsJSON();
                      }}
                      title="Download research logs"
                    >
                      <Icon icon={Download} size={14} />
                      Logs
                    </button>
                    <div className="flex items-center justify-center text-text-muted transition-transform duration-200 ease-out">
                      <Icon
                        icon={detailsExpanded ? ChevronDown : ChevronRight}
                        size={16}
                      />
                    </div>
                  </div>
                </div>

                {detailsExpanded && (
                  <div className="bg-background-secondary">
                    {/* Activity Log Snapshot */}
                    {effectiveState.completionSnapshot?.activityLog &&
                      effectiveState.completionSnapshot.activityLog.length > 0 && (
                        <div className="px-3.5 py-3 border-b border-border last:border-b-0">
                          <div className="flex items-center justify-between mb-2.5">
                            <span className="flex items-center gap-1.5 text-xs font-semibold text-text uppercase tracking-[0.5px]">
                              <Icon icon={ListTodo} size={14} />
                              Activity Timeline
                            </span>
                            <span className="text-[11px] text-text-muted font-normal">
                              {effectiveState.completionSnapshot.stepsTaken} steps
                              {effectiveState.completionSnapshot.elapsedTime &&
                                ` · ${Math.round(effectiveState.completionSnapshot.elapsedTime / 1000)}s`}
                            </span>
                          </div>
                          <div className="flex flex-col gap-1.5 text-xs text-text-secondary">
                            {effectiveState.completionSnapshot.activityLog.map(
                              (event, i) => (
                                <div key={i} className="flex items-start gap-1.5 px-2 py-1.5 bg-background-tertiary rounded leading-[1.4]">
                                  {event}
                                </div>
                              )
                            )}
                          </div>
                        </div>
                      )}

                    {/* Research plan */}
                    <ResearchPlanSection
                      questions={effectiveState.researchPlan}
                      facts={effectiveState.gatheredFacts}
                      onSkipQuestion={onSkipQuestion}
                      isRunning={false}
                      isCompleted={true}
                    />

                    {/* Gathered facts */}
                    <GatheredFactsSection facts={effectiveState.gatheredFacts} />

                    {/* Working hypothesis */}
                    <HypothesisBlock hypothesis={effectiveState.currentHypothesis} />

                    {/* Previous rounds (if multi-round research) */}
                    <PreviousRoundsSection
                      roundSummaries={effectiveState.roundSummaries}
                    />

                    {/* Knowledge gaps */}
                    <KnowledgeGapsSection gaps={effectiveState.knowledgeGaps} />
                  </div>
                )}
              </div>
            </>
          ) : (
            <>
              {/* Research plan */}
              <ResearchPlanSection
                questions={effectiveState.researchPlan}
                facts={effectiveState.gatheredFacts}
                onSkipQuestion={onSkipQuestion}
                onSkipAllPending={onSkipAllPending}
                onAddQuestion={onAddQuestion}
                onGenerateMoreQuestions={onGenerateMoreQuestions}
                onExpandQuestion={onExpandQuestion}
                onGoDeeper={onGoDeeper}
                onForceAnswer={onForceAnswer}
                isRunning={isRunning}
              />

              {/* Gathered facts */}
              <GatheredFactsSection facts={effectiveState.gatheredFacts} />

              {/* Working hypothesis */}
              <HypothesisBlock hypothesis={effectiveState.currentHypothesis} />

              {/* Previous rounds (if multi-round research) */}
              <PreviousRoundsSection
                roundSummaries={effectiveState.roundSummaries}
              />

              {/* Knowledge gaps */}
              <KnowledgeGapsSection gaps={effectiveState.knowledgeGaps} />

              {/* Error message if failed */}
              {effectiveState.phase === 'error' && effectiveState.errorMessage && (
                <div className="px-3.5 py-3 border-b border-border last:border-b-0">
                  <div className="px-3 py-2.5 bg-[rgba(239,68,68,0.1)] border border-[rgba(239,68,68,0.3)] rounded-md">
                    <div className="text-[11px] font-semibold text-[#f87171] mb-1.5">Error</div>
                    <div className="text-xs text-text leading-[1.4]">
                      {effectiveState.errorMessage}
                    </div>
                  </div>
                </div>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
};

export default ResearchArtifact;
