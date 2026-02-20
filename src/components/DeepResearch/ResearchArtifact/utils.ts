/**
 * Pure helper functions for the ResearchArtifact component.
 *
 * @module components/DeepResearch/ResearchArtifact/utils
 */

import type { ResearchState, ResearchPhase } from './types';

// =============================================================================
// getLiveActivity
// =============================================================================

/**
 * Derive the "live activity" text from current state.
 * Shows what the agent is currently doing.
 */
export function getLiveActivity(state: ResearchState, isRunning: boolean): string {
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

// =============================================================================
// getPhaseConfig
// =============================================================================

/**
 * Get phase badge styling and label.
 */
export function getPhaseConfig(phase: ResearchPhase): { label: string; className: string } {
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

// =============================================================================
// calculateProgress
// =============================================================================

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
export function calculateProgress(state: ResearchState): number {
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
