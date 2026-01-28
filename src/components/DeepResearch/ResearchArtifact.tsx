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
import styles from './ResearchArtifact.module.css';

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
      return { label: 'Planning', className: styles.phasePlanning };
    case 'gathering':
      return { label: 'Gathering', className: styles.phaseGathering };
    case 'evaluating':
      return { label: 'Evaluating', className: styles.phaseEvaluating };
    case 'compressing':
      return { label: 'Compressing', className: styles.phaseCompressing };
    case 'synthesizing':
      return { label: 'Synthesizing', className: styles.phaseSynthesizing };
    case 'complete':
      return { label: 'Complete', className: styles.phaseComplete };
    case 'error':
      return { label: 'Error', className: styles.phaseError };
  }
}

/**
 * Get question status icon.
 */
function QuestionStatusIcon({ status }: { status: QuestionStatus }) {
  switch (status) {
    case 'pending':
      return <Icon icon={Circle} size={16} className={styles.statusPending} />;
    case 'in-progress':
      return <Icon icon={Loader2} size={16} className={styles.statusInProgress} />;
    case 'answered':
      return <Icon icon={CircleCheck} size={16} className={styles.statusAnswered} />;
    case 'blocked':
      return <Icon icon={CircleX} size={16} className={styles.statusBlocked} />;
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
    <div className={styles.thinkingBlock}>
      <div className={styles.thinkingHeader}>
        <Icon icon={Brain} size={12} />
        <span>Thinking</span>
      </div>
      <div className={styles.thinkingContent}>{reasoning}</div>
    </div>
  );
};

/**
 * Research plan section showing questions and their status.
 * Enhanced with user controls for adding questions and AI-directed actions.
 */
const ResearchPlanSection: React.FC<{
  questions: ResearchQuestion[];
  onSkipQuestion?: (questionId: string) => void;
  onSkipAllPending?: () => void;
  onAddQuestion?: (question: string) => void;
  onGenerateMoreQuestions?: () => void;
  onExpandQuestion?: (questionId: string) => void;
  onGoDeeper?: () => void;
  isRunning: boolean;
  isCompleted?: boolean;
}> = ({
  questions,
  onSkipQuestion,
  onSkipAllPending,
  onAddQuestion,
  onGenerateMoreQuestions,
  onExpandQuestion,
  onGoDeeper,
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
  
  // Count pending questions
  const pendingCount = questions.filter(
    q => q.status === 'pending' || q.status === 'in-progress'
  ).length;
  
  // Check if actions are available (only during gathering phase when running)
  const actionsAvailable = !isCompleted && isRunning;
  
  if (questions.length === 0) {
    return (
      <div className={styles.section}>
        <div className={styles.sectionHeader}>
          <span className={styles.sectionTitle}>
            <Icon icon={ListTodo} size={14} />
            Research Plan
          </span>
        </div>
        <div className={styles.emptyState}>No research questions yet...</div>
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
    <div className={styles.section}>
      <div className={styles.sectionHeader}>
        <span className={styles.sectionTitle}>
          <Icon icon={ListTodo} size={14} />
          Research Plan
        </span>
        <span className={styles.sectionCount}>
          {questions.filter(q => q.status === 'answered').length}/{questions.length} answered
        </span>
      </div>
      
      {/* Action buttons row - only show when research is running */}
      {actionsAvailable && (
        <div className={styles.questionActions}>
          {/* Add question button/input */}
          {showAddInput ? (
            <div className={styles.addQuestionInput}>
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
                className={styles.addQuestionField}
                autoFocus
              />
              <button
                className={styles.addQuestionSubmit}
                onClick={handleAddQuestion}
                disabled={!newQuestionText.trim()}
                title="Add question"
                type="button"
              >
                <Icon icon={Plus} size={14} />
              </button>
              <button
                className={styles.addQuestionCancel}
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
              className={styles.actionButton}
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
            className={styles.actionButton}
            onClick={handleGenerateMore}
            disabled={isGenerating}
            title="Ask AI to generate more research questions"
            type="button"
          >
            {isGenerating ? (
              <Icon icon={Loader2} size={12} className={styles.spinIcon} />
            ) : (
              <Icon icon={Wand2} size={12} />
            )}
            More Questions
          </button>
          
          <button
            className={styles.actionButton}
            onClick={handleGoDeeper}
            disabled={isGoingDeeper}
            title="Ask AI to explore deeper based on current findings"
            type="button"
          >
            {isGoingDeeper ? (
              <Icon icon={Loader2} size={12} className={styles.spinIcon} />
            ) : (
              <Icon icon={ArrowDownToLine} size={12} />
            )}
            Go Deeper
          </button>
          
          {/* Skip all pending - only show if there are multiple pending questions */}
          {pendingCount > 1 && onSkipAllPending && (
            <button
              className={`${styles.actionButton} ${styles.actionButtonDanger}`}
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
      
      <div className={styles.questionList}>
        {sortedQuestions.map(question => {
          const canSkip = actionsAvailable && 
            onSkipQuestion && 
            (question.status === 'in-progress' || question.status === 'pending') &&
            !pendingSkips.has(question.id);
          
          const canExpand = actionsAvailable &&
            onExpandQuestion &&
            question.status === 'pending' &&
            !pendingExpands.has(question.id);
          
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
              className={`${styles.questionItem} ${isBlocked ? styles.questionBlocked : ''}`}
            >
              <div className={styles.questionStatus}>
                <QuestionStatusIcon status={question.status} />
              </div>
              <div className={styles.questionContent}>
                <div className={styles.questionText}>
                  {sourceIcon && (
                    <span className={styles.questionSourceIcon} title={`Source: ${question.source}`}>
                      <Icon icon={sourceIcon} size={10} />
                    </span>
                  )}
                  {question.question}
                </div>
                {question.answerSummary && (
                  <div className={styles.questionAnswer}>{question.answerSummary}</div>
                )}
              </div>
              <div className={styles.questionButtons}>
                {/* Expand button */}
                {canExpand && (
                  <button
                    className={styles.expandButton}
                    onClick={() => handleExpand(question.id)}
                    title="Ask AI to break this into sub-questions"
                    type="button"
                  >
                    {pendingExpands.has(question.id) ? (
                      <Icon icon={Loader2} size={12} className={styles.spinIcon} />
                    ) : (
                      <Icon icon={Maximize2} size={12} />
                    )}
                  </button>
                )}
                {/* Skip button */}
                {showSkipButton && (
                  <button
                    className={styles.skipButton}
                    onClick={() => handleSkip(question.id)}
                    title={isCompleted ? "Question was skipped" : "Skip this question"}
                    type="button"
                    disabled={isCompleted}
                  >
                    <Icon icon={SkipForward} size={12} />
                  </button>
                )}
                {pendingSkips.has(question.id) && (
                  <div className={styles.skipPending}>
                    <Icon icon={Loader2} size={12} className={styles.skipSpinner} />
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
      <div className={styles.section}>
        <div className={styles.sectionHeader}>
          <span className={styles.sectionTitle}>
            <Icon icon={FileSearch} size={14} />
            Gathered Facts
          </span>
        </div>
        <div className={styles.emptyState}>No facts gathered yet...</div>
      </div>
    );
  }

  // Sort by most recent first
  const sortedFacts = [...facts].sort((a, b) => b.gatheredAtStep - a.gatheredAtStep);
  const displayedFacts = showAll ? sortedFacts : sortedFacts.slice(0, displayLimit);

  return (
    <div className={styles.section}>
      <div className={styles.sectionHeader}>
        <span className={styles.sectionTitle}>
          <Icon icon={FileSearch} size={14} />
          Gathered Facts
        </span>
        <span className={styles.sectionCount}>{facts.length} facts</span>
      </div>
      <div className={styles.factList}>
        {displayedFacts.map(fact => (
          <div
            key={fact.id}
            className={styles.factItem}
            data-confidence={fact.confidence}
          >
            <div className={styles.factClaim}>{fact.claim}</div>
            <div className={styles.factMeta}>
              <a
                href={fact.sourceUrl}
                target="_blank"
                rel="noopener noreferrer"
                className={styles.factSource}
                title={fact.sourceUrl}
              >
                <Icon icon={ExternalLink} size={10} />
                {fact.sourceTitle || new URL(fact.sourceUrl).hostname}
              </a>
              <span className={styles.factConfidence}>
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
          className={styles.showMoreButton}
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
    <div className={styles.section}>
      <div className={styles.hypothesisBlock}>
        <div className={styles.hypothesisLabel}>
          <Icon icon={Lightbulb} size={12} />
          <span>Working Hypothesis</span>
        </div>
        <div className={styles.hypothesisText}>{hypothesis}</div>
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
    <div className={styles.section}>
      <div className={styles.sectionHeader}>
        <span className={styles.sectionTitle}>
          <Icon icon={Search} size={14} />
          Knowledge Gaps
        </span>
      </div>
      <div className={styles.gapList}>
        {gaps.map((gap, idx) => (
          <span key={idx} className={styles.gapItem}>
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
    <div className={styles.section}>
      <div
        className={styles.sectionHeader}
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
        <span className={styles.sectionTitle}>
          <Icon icon={History} size={14} />
          Previous Rounds
        </span>
        <span className={styles.sectionCount}>
          {roundSummaries.length} round{roundSummaries.length !== 1 ? 's' : ''}
          <Icon
            icon={expanded ? ChevronDown : ChevronRight}
            size={14}
            style={{ marginLeft: 4 }}
          />
        </span>
      </div>
      {expanded && (
        <div className={styles.previousRoundsContent}>
          {roundSummaries.map((round) => (
            <div key={round.round} className={styles.roundSummaryCard}>
              <div className={styles.roundSummaryHeader}>
                <span className={styles.roundNumber}>
                  <Icon icon={Layers} size={12} />
                  Round {round.round}
                  {round.perspective && (
                    <span className={styles.roundPerspective}>
                      ({round.perspective})
                    </span>
                  )}
                </span>
                <span className={styles.roundMeta}>
                  {round.factCountAtEnd} facts · {round.questionsAnsweredThisRound.length} questions
                </span>
              </div>
              <div className={styles.roundSummaryText}>
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
    <div className={styles.finalReport}>
      <div className={styles.reportContent}>{renderedReport}</div>
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
    <span className={styles.citationRef} tabIndex={0} role="button">
      [{number}]
      <span className={styles.citationHoverCard}>
        <div className={styles.hoverCardHeader}>
          <span className={styles.hoverCardNumber}>{number}</span>
          <span className={styles.hoverCardTitle}>{fact.sourceTitle}</span>
        </div>
        <div className={styles.hoverCardClaim}>{displayClaim}</div>
        <div className={styles.hoverCardMeta}>
          <span
            className={styles.hoverCardConfidence}
            data-confidence={fact.confidence}
          >
            <CheckCircle2 size={12} />
            {fact.confidence} confidence
          </span>
          <a
            href={fact.sourceUrl}
            target="_blank"
            rel="noopener noreferrer"
            className={styles.hoverCardLink}
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
      className={`${styles.artifactContainer} ${className || ''}`}
      data-running={isRunning}
    >
      {/* Header (always visible) */}
      <div
        className={styles.header}
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
        <div className={styles.headerIcon}>
          <Icon icon={Sparkles} size={18} />
        </div>

        <div className={styles.headerContent}>
          <div className={styles.headerTitle}>
            <span>Deep Research</span>
            <span className={`${styles.phaseBadge} ${phaseConfig.className}`}>
              {phaseConfig.label}
            </span>
            {effectiveState.maxRounds > 1 && (
              <span className={styles.roundBadge}>
                Round {effectiveState.currentRound}/{effectiveState.maxRounds}
              </span>
            )}
            {effectiveState.currentPerspective && (
              <span className={styles.perspectiveBadge}>
                🎭 {effectiveState.currentPerspective}
              </span>
            )}
          </div>

          <div className={styles.liveActivity}>
            {isRunning && (
              <Icon icon={Loader2} size={12} className={styles.activitySpinner} />
            )}
            <span>{liveActivity}</span>
          </div>
        </div>

        {/* Download logs button - always visible in header */}
        <button
          className={styles.headerDownloadButton}
          onClick={(e) => {
            e.stopPropagation();
            downloadAsJSON();
          }}
          title="Download research logs (JSON)"
        >
          <Icon icon={Download} size={14} />
        </button>

        <div className={styles.expandToggle} data-expanded={expanded}>
          <Icon icon={ChevronDown} size={18} />
        </div>
      </div>

      {/* Progress bar (always visible when not complete) */}
      {effectiveState.phase !== 'complete' && effectiveState.phase !== 'error' && (
        <div className={styles.progressContainer}>
          <div className={styles.progressBar}>
            <div
              className={styles.progressFill}
              style={{ width: `${progress}%` }}
            />
          </div>
          <div className={styles.progressLabel}>
            <span>Step {effectiveState.currentStep}/{effectiveState.maxSteps}</span>
            <span>{progress}%</span>
          </div>
        </div>
      )}

      {/* Activity log (trailing events for velocity visibility) */}
      {isRunning && effectiveState.activityLog && effectiveState.activityLog.length > 0 && (
        <div className={styles.activityLog}>
          {effectiveState.activityLog.map((entry, idx) => {
            const isSkipped = entry.startsWith('Search skipped:') || entry.includes('(duplicate)');
            return (
              <div
                key={idx}
                className={`${styles.activityEntry} ${isSkipped ? styles.activitySkipped : ''}`}
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
        <div className={styles.collapsedSummary}>
          <div className={styles.summaryStats}>
            <span className={styles.summaryStat}>
              <strong>{effectiveState.researchPlan.length}</strong> questions
            </span>
            <span className={styles.summaryStat}>
              <strong>{effectiveState.gatheredFacts.length}</strong> facts
            </span>
            {effectiveState.knowledgeGaps.length > 0 && (
              <span className={styles.summaryStat}>
                <strong>{effectiveState.knowledgeGaps.length}</strong> gaps
              </span>
            )}
          </div>
        </div>
      )}

      {/* Expanded content */}
      {expanded && (
        <div className={styles.expandedContent}>
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
              <div className={styles.researchDetailsContainer}>
                <div
                  className={styles.researchDetailsHeader}
                  onClick={() => setDetailsExpanded(!detailsExpanded)}
                >
                  <div className={styles.researchDetailsTitle}>
                    <Icon icon={FileSearch} size={14} />
                    Research Details
                  </div>
                  <div className={styles.researchDetailsActions}>
                    <button
                      className={styles.downloadLogsButton}
                      onClick={(e) => {
                        e.stopPropagation();
                        downloadAsJSON();
                      }}
                      title="Download research logs"
                    >
                      <Icon icon={Download} size={14} />
                      Logs
                    </button>
                    <div className={styles.researchDetailsToggle}>
                      <Icon
                        icon={detailsExpanded ? ChevronDown : ChevronRight}
                        size={16}
                      />
                    </div>
                  </div>
                </div>

                {detailsExpanded && (
                  <div className={styles.researchDetailsContent}>
                    {/* Activity Log Snapshot */}
                    {effectiveState.completionSnapshot?.activityLog &&
                      effectiveState.completionSnapshot.activityLog.length > 0 && (
                        <div className={styles.section}>
                          <div className={styles.sectionHeader}>
                            <span className={styles.sectionTitle}>
                              <Icon icon={ListTodo} size={14} />
                              Activity Timeline
                            </span>
                            <span className={styles.sectionCount}>
                              {effectiveState.completionSnapshot.stepsTaken} steps
                              {effectiveState.completionSnapshot.elapsedTime &&
                                ` · ${Math.round(effectiveState.completionSnapshot.elapsedTime / 1000)}s`}
                            </span>
                          </div>
                          <div className={styles.activityLogSnapshot}>
                            {effectiveState.completionSnapshot.activityLog.map(
                              (event, i) => (
                                <div key={i} className={styles.activityItem}>
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
                onSkipQuestion={onSkipQuestion}
                onSkipAllPending={onSkipAllPending}
                onAddQuestion={onAddQuestion}
                onGenerateMoreQuestions={onGenerateMoreQuestions}
                onExpandQuestion={onExpandQuestion}
                onGoDeeper={onGoDeeper}
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
                <div className={styles.section}>
                  <div className={styles.contradictionItem}>
                    <div className={styles.contradictionLabel}>Error</div>
                    <div className={styles.contradictionText}>
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
