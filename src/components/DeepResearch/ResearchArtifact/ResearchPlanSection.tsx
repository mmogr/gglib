import React, { useState } from 'react';
import {
  ListTodo,
  Plus,
  Wand2,
  ArrowDownToLine,
  SkipForward,
  Loader2,
  Maximize2,
  User,
  Bot,
  Zap,
} from 'lucide-react';
import { Icon } from '../../ui/Icon';
import { cn } from '../../../utils/cn';
import { ACTION_BTN_STYLES } from './styles';
import { QuestionStatusIcon } from './QuestionStatusIcon';
import type { ResearchQuestion, GatheredFact, QuestionStatus } from './types';

interface ResearchPlanSectionProps {
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
}

/**
 * Research plan — questions list with skip/expand/add/force-answer/go-deeper controls.
 * Uses local `useState` for optimistic UI on skip, expand, force-answer, etc.
 */
const ResearchPlanSection: React.FC<ResearchPlanSectionProps> = ({
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
              className={ACTION_BTN_STYLES}
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
            className={ACTION_BTN_STYLES}
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
            className={ACTION_BTN_STYLES}
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
              className={cn(
                ACTION_BTN_STYLES,
                'text-[#f87171] hover:text-[#f87171] hover:bg-[rgba(248,113,113,0.1)] hover:border-[rgba(248,113,113,0.3)]',
              )}
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

ResearchPlanSection.displayName = 'ResearchPlanSection';

export { ResearchPlanSection };
