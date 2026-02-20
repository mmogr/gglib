import React, { useState, useMemo } from 'react';
import {
  Sparkles,
  ChevronDown,
  ChevronRight,
  Loader2,
  FileSearch,
  Download,
  ListTodo,
} from 'lucide-react';
import { Icon } from '../../ui/Icon';
import { cn } from '../../../utils/cn';
import { useResearchLogExport } from '../../../hooks/useResearchLogs';
import { getPhaseConfig, getLiveActivity, calculateProgress } from './utils';
import { ACTION_BTN_STYLES } from './styles';
import { ThinkingBlock } from './ThinkingBlock';
import { FinalReportSection } from './FinalReportSection';
import { ResearchPlanSection } from './ResearchPlanSection';
import { GatheredFactsSection } from './GatheredFactsSection';
import { HypothesisBlock } from './HypothesisBlock';
import { PreviousRoundsSection } from './PreviousRoundsSection';
import { KnowledgeGapsSection } from './KnowledgeGapsSection';
import type { ResearchArtifactProps } from './types';

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
          <ThinkingBlock lastReasoning={effectiveState.lastReasoning} />

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
                      className={cn(
                        ACTION_BTN_STYLES,
                        'bg-background-secondary hover:border-border-hover active:scale-[0.97]',
                      )}
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

ResearchArtifact.displayName = 'ResearchArtifact';

export default ResearchArtifact;
