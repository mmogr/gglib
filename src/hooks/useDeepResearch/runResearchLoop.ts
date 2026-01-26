/**
 * Deep Research Loop Orchestrator
 *
 * Implements the Plan-and-Execute state machine with:
 * - "Soft Landing" termination guardrail (force synthesis at 80% steps)
 * - Parallel batch tool execution (Promise.all)
 * - Tool failure resilience (errors become observations, not crashes)
 *
 * @module useDeepResearch/runResearchLoop
 */

import type {
  ResearchState,
  GatheredFact,
  ResearchQuestion,
  PendingObservation,
  ModelRouting,
  ModelEndpoint,
  InterventionRef,
  ResearchIntervention,
  ActiveToolCall,
} from './types';
import {
  createInitialState,
  createQuestion,
  createFact,
  addFacts,
  updateQuestion,
  addObservation,
  clearObservations,
  advanceStep,
  setPhase,
  setError,
  completeResearch,
  pushActivityLog,
  setActiveToolCalls,
  clearActiveToolCalls,
  setLLMGenerating,
  // Multi-round helpers
  isSearchDuplicate,
  addSearchRecord,
  createRoundSummary,
  advanceRound,
  canContinueResearch,
  shouldTriggerRoundSoftLanding,
  getRoundStepBudget,
  // Internal tool helpers
  isInternalResearchTool,
  MIN_FACTS_FOR_SYNTHESIS,
} from './types';
import {
  buildTurnMessagesWithBudget,
  shouldIncludeTools,
  getResearchToolsWithInternals,
  PHASE_INSTRUCTIONS,
  type TurnMessage,
} from './buildTurnMessages';

// =============================================================================
// Configuration
// =============================================================================

/** Default maximum research steps before hard stop */
export const DEFAULT_MAX_STEPS = 30;

/** Soft landing threshold - force synthesis at this percentage of max steps */
export const SOFT_LANDING_THRESHOLD = 0.8;

/** Maximum concurrent tool calls in a batch */
export const MAX_PARALLEL_TOOLS = 5;

/** Tool execution timeout (ms) */
export const TOOL_TIMEOUT_MS = 30000;

/** Maximum retries for transient errors */
export const MAX_TOOL_RETRIES = 2;

/** Maximum steps a question can be in-progress before being marked blocked */
export const QUESTION_FOCUS_TIMEOUT_STEPS = 5;

// =============================================================================
// Types
// =============================================================================

/**
 * Tool definition (OpenAI-compatible format).
 */
export interface ToolDefinition {
  type: 'function';
  function: {
    name: string;
    description?: string;
    parameters?: Record<string, unknown>;
  };
}

/**
 * Tool call from LLM response.
 */
export interface ToolCall {
  id: string;
  type: 'function';
  function: {
    name: string;
    arguments: string;
  };
}

/**
 * Result from tool execution.
 */
export type ToolResult =
  | { success: true; data: unknown }
  | { success: false; error: string };

/**
 * Tool executor function signature.
 */
export type ToolExecutor = (
  name: string,
  args: Record<string, unknown>
) => Promise<ToolResult>;

/**
 * LLM response from streaming/completion.
 */
export interface LLMResponse {
  content: string;
  toolCalls: ToolCall[];
  finishReason: 'stop' | 'tool_calls' | 'length' | 'error';
}

/**
 * LLM caller function signature.
 * Abstracted to allow different backends (proxy, direct API, etc.)
 */
export type LLMCaller = (
  messages: TurnMessage[],
  options: {
    tools?: ToolDefinition[];
    endpoint: ModelEndpoint;
    abortSignal?: AbortSignal;
  }
) => Promise<LLMResponse>;

/**
 * Options for running the research loop.
 */
export interface RunResearchLoopOptions {
  /** The user's research query */
  query: string;
  /** Message ID for persistence */
  messageId: string;
  /** Conversation ID (optional) */
  conversationId?: number;
  /** Model routing configuration */
  modelRouting: ModelRouting;
  /** Base system prompt (safety/personality) */
  baseSystemPrompt: string;
  /** Available tool definitions */
  tools: ToolDefinition[];
  /** Tool executor function */
  executeTool: ToolExecutor;
  /** LLM caller function */
  callLLM: LLMCaller;
  /** Maximum research steps (default: 30) */
  maxSteps?: number;
  /** Callback for state updates (for UI) */
  onStateUpdate?: (state: ResearchState) => void;
  /** Callback for state persistence */
  onStatePersist?: (state: ResearchState) => Promise<void>;
  /** Abort signal for cancellation */
  abortSignal?: AbortSignal;
  /** Token budget for context (default: 8000) */
  maxContextTokens?: number;
  /** 
   * Ref for human-in-the-loop intervention signals.
   * UI writes to this ref, loop reads at start of each step.
   * Supports 'wrap-up' (force synthesis) and 'skip-question' (mark blocked).
   */
  interventionRef?: InterventionRef;
}

/**
 * Result from running the research loop.
 */
export interface ResearchLoopResult {
  /** Final state */
  state: ResearchState;
  /** Whether research completed successfully */
  success: boolean;
  /** Error message if failed */
  error?: string;
}

// =============================================================================
// Structured Response Parsing
// =============================================================================

/**
 * Planning phase response.
 */
interface PlanResponse {
  type: 'plan';
  hypothesis: string;
  questions: Array<{ question: string; priority: number }>;
  gaps?: string[];
  /** Query complexity classification (adaptive planner) */
  complexity?: 'simple' | 'multi-faceted' | 'controversial';
  /** Research perspectives for multi-faceted/controversial queries */
  perspectives?: string[];
}

/**
 * Gathering phase response (when answering, not tool calling).
 * Now supports questionIndex (preferred) or questionId (legacy).
 */
interface AnswerResponse {
  type: 'answer';
  /** Preferred: 1-based index matching Q1, Q2, etc. from Research Plan */
  questionIndex?: number;
  /** Legacy: UUID of question (kept for backwards compatibility) */
  questionId?: string;
  answer: string;
  facts: Array<{
    claim: string;
    sourceUrl: string;
    sourceTitle: string;
    confidence: 'high' | 'medium' | 'low';
  }>;
  updatedHypothesis?: string;
  newGaps?: string[];
}

/**
 * Synthesis phase response.
 */
interface ReportResponse {
  type: 'report';
  report: string;
  citations: Array<{ factId: string; footnoteNumber: number }>;
  confidence?: 'high' | 'medium' | 'low';
  limitations?: string[];
}

/**
 * Evaluation phase response (multi-round support).
 */
interface EvaluationResponse {
  type: 'evaluation';
  adequacyScore: number; // 1-10 scale
  assessment: string;
  missingAspects: string[];
  suggestedFollowups: Array<{
    question: string;
    priority: number;
    rationale: string;
  }>;
  shouldContinue: boolean;
}

/**
 * Compression phase response (round summary).
 */
interface RoundSummaryResponse {
  type: 'roundSummary';
  summary: string;
  keyInsights: string[];
}

type StructuredResponse = PlanResponse | AnswerResponse | ReportResponse | EvaluationResponse | RoundSummaryResponse;

/**
 * Try to parse structured JSON from LLM content.
 */
function tryParseStructuredResponse(content: string): StructuredResponse | null {
  const trimmed = content.trim();
  
  // Try to extract JSON from markdown code blocks
  const jsonMatch = trimmed.match(/```(?:json)?\s*([\s\S]*?)```/);
  const jsonStr = jsonMatch ? jsonMatch[1].trim() : trimmed;
  
  // Only parse if it looks like JSON
  if (!jsonStr.startsWith('{')) {
    return null;
  }
  
  try {
    const parsed = JSON.parse(jsonStr);
    
    // Validate type field
    if (!parsed || typeof parsed !== 'object' || !('type' in parsed)) {
      return null;
    }
    
    const validTypes = ['plan', 'answer', 'report', 'evaluation', 'roundSummary'];
    if (!validTypes.includes(parsed.type)) {
      return null;
    }
    
    return parsed as StructuredResponse;
  } catch {
    return null;
  }
}

// =============================================================================
// Intelligent Synthesis Decision Logic
// =============================================================================

/**
 * Result from the synthesis readiness calculation.
 * Contains all factors used to make the decision + reasoning.
 */
interface SynthesisDecision {
  shouldSynthesize: boolean;
  reason: string;
  /** Raw evaluation score from LLM */
  rawScore: number;
  /** Score adjusted for perspective coverage gaps */
  adjustedScore: number;
  /** Threshold required for synthesis (varies by complexity) */
  threshold: number;
  /** Fraction of declared perspectives that have been researched */
  perspectiveCoverage: number;
  /** Diversity score based on fact distribution across perspectives */
  factDiversity: number;
}

/**
 * Calculate whether research is ready for synthesis.
 * 
 * This implements adaptive, intelligent decision-making based on:
 * 1. Query complexity (simple vs multi-faceted vs controversial)
 * 2. Perspective coverage (have we explored all declared angles?)
 * 3. Fact diversity (are facts distributed across perspectives?)
 * 4. Research depth (facts per perspective)
 * 
 * For simple queries: Standard threshold of 7 applies
 * For multi-faceted: Requires exploring majority of perspectives
 * For controversial: Requires balanced coverage of opposing viewpoints
 * 
 * The score is adjusted downward if perspective coverage is insufficient,
 * effectively requiring more rounds before synthesis can occur.
 */
function calculateSynthesisReadiness(
  state: ResearchState,
  rawScore: number,
  modelShouldContinue: boolean
): SynthesisDecision {
  const { complexity, perspectives, roundSummaries, gatheredFacts, currentRound, maxRounds } = state;
  const canContinue = canContinueResearch(state);
  
  // === Calculate perspective coverage ===
  // Which perspectives have we actually researched (based on round summaries)?
  const exploredPerspectives = new Set<string>();
  for (const summary of roundSummaries) {
    if (summary.perspective) {
      exploredPerspectives.add(summary.perspective.toLowerCase().trim());
    }
  }
  // Also count current perspective if we're in round 1+ and have gathered facts
  if (state.currentPerspective && gatheredFacts.length > 0) {
    exploredPerspectives.add(state.currentPerspective.toLowerCase().trim());
  }
  
  const totalPerspectives = perspectives.length || 1;
  const perspectiveCoverage = totalPerspectives > 0 
    ? exploredPerspectives.size / totalPerspectives 
    : 1.0;
  
  // === Calculate fact diversity ===
  // For multi-perspective topics, we want facts from different angles
  // Heuristic: count facts per perspective based on round they were gathered
  const factsPerRound: Map<number, number> = new Map();
  for (const fact of gatheredFacts) {
    // Approximate which round this fact is from based on step
    const roundBoundaries = calculateRoundBoundaries(maxRounds, state.maxSteps);
    const factRound = roundBoundaries.findIndex((end, idx) => 
      fact.gatheredAtStep <= end && (idx === 0 || fact.gatheredAtStep > roundBoundaries[idx - 1])
    ) + 1 || 1;
    
    factsPerRound.set(factRound, (factsPerRound.get(factRound) || 0) + 1);
  }
  
  // Diversity score: entropy-based measure of fact distribution
  // Higher when facts are spread across rounds (perspectives)
  const totalFacts = gatheredFacts.length || 1;
  let entropy = 0;
  for (const count of factsPerRound.values()) {
    const p = count / totalFacts;
    if (p > 0) entropy -= p * Math.log2(p);
  }
  // Normalize to 0-1 scale (max entropy for N rounds is log2(N))
  const maxEntropy = totalPerspectives > 1 ? Math.log2(totalPerspectives) : 1;
  const factDiversity = maxEntropy > 0 ? Math.min(1, entropy / maxEntropy) : 1.0;
  
  // === Determine thresholds based on complexity ===
  let baseThreshold: number;
  let minPerspectiveCoverage: number;
  
  switch (complexity) {
    case 'controversial':
      // Controversial topics need balanced coverage of opposing views
      baseThreshold = 8; // Higher bar
      minPerspectiveCoverage = 0.67; // Need at least 2/3 of perspectives
      break;
    case 'multi-faceted':
      // Multi-faceted topics need good breadth
      baseThreshold = 7;
      minPerspectiveCoverage = 0.5; // Need at least half of perspectives
      break;
    case 'simple':
    default:
      // Simple topics can synthesize more readily
      baseThreshold = 7;
      minPerspectiveCoverage = 0; // No perspective requirement
      break;
  }
  
  // === Calculate adjusted score ===
  // Penalize the score if perspective coverage is insufficient
  let adjustedScore = rawScore;
  let coverageDeficit = '';
  
  if (complexity !== 'simple' && perspectiveCoverage < minPerspectiveCoverage && canContinue) {
    // Apply progressive penalty based on coverage gap
    const coverageGap = minPerspectiveCoverage - perspectiveCoverage;
    const penalty = Math.min(3, coverageGap * 5); // Up to 3-point penalty
    adjustedScore = Math.max(1, rawScore - penalty);
    
    const explored = exploredPerspectives.size;
    const needed = Math.ceil(totalPerspectives * minPerspectiveCoverage);
    coverageDeficit = `(explored ${explored}/${totalPerspectives} perspectives, need ${needed})`;
  }
  
  // Also penalize if fact diversity is very low for complex topics
  if (complexity !== 'simple' && factDiversity < 0.3 && currentRound === 1 && canContinue && totalPerspectives > 1) {
    // Low diversity on first round - nudge toward more research
    adjustedScore = Math.max(1, adjustedScore - 1);
  }
  
  // === Make the decision ===
  // Hard stops that bypass score-based logic
  if (!canContinue) {
    return {
      shouldSynthesize: true,
      reason: `Maximum rounds (${maxRounds}) reached`,
      rawScore,
      adjustedScore: rawScore,
      threshold: baseThreshold,
      perspectiveCoverage,
      factDiversity,
    };
  }
  
  // Check if model explicitly says to stop (unless we have coverage issues)
  if (!modelShouldContinue && perspectiveCoverage >= minPerspectiveCoverage) {
    return {
      shouldSynthesize: true,
      reason: 'Model indicates research complete',
      rawScore,
      adjustedScore,
      threshold: baseThreshold,
      perspectiveCoverage,
      factDiversity,
    };
  }
  
  // Score-based decision with adjusted score
  if (adjustedScore >= baseThreshold) {
    // Check for minimum perspective coverage override
    if (complexity !== 'simple' && perspectiveCoverage < minPerspectiveCoverage) {
      return {
        shouldSynthesize: false,
        reason: `Score ${rawScore}/10 but insufficient perspective coverage ${coverageDeficit}`,
        rawScore,
        adjustedScore,
        threshold: baseThreshold,
        perspectiveCoverage,
        factDiversity,
      };
    }
    
    return {
      shouldSynthesize: true,
      reason: `Research quality sufficient (${adjustedScore}/10)`,
      rawScore,
      adjustedScore,
      threshold: baseThreshold,
      perspectiveCoverage,
      factDiversity,
    };
  }
  
  // Score below threshold - continue researching
  return {
    shouldSynthesize: false,
    reason: `Score ${adjustedScore}/10 < ${baseThreshold} threshold`,
    rawScore,
    adjustedScore,
    threshold: baseThreshold,
    perspectiveCoverage,
    factDiversity,
  };
}

/**
 * Calculate step boundaries for each round based on 60/30/10 split.
 * Returns cumulative end steps for each round.
 */
function calculateRoundBoundaries(maxRounds: number, maxSteps: number): number[] {
  if (maxRounds === 1) return [maxSteps];
  if (maxRounds === 2) return [Math.floor(maxSteps * 0.7), maxSteps];
  
  // 60/30/10 split for 3 rounds
  const r1End = Math.floor(maxSteps * 0.6);
  const r2End = r1End + Math.floor(maxSteps * 0.3);
  const r3End = maxSteps;
  
  const boundaries = [r1End, r2End, r3End];
  
  // Add extra rounds if needed
  for (let i = 3; i < maxRounds; i++) {
    boundaries.push(maxSteps);
  }
  
  return boundaries;
}

// =============================================================================
// Tool Execution with Resilience
// =============================================================================

/**
 * Execute a single tool with timeout and error handling.
 * Errors are converted to failed observations, not thrown.
 */
async function executeToolSafely(
  toolCall: ToolCall,
  executeTool: ToolExecutor,
  forQuestionId?: string
): Promise<PendingObservation> {
  const startTime = Date.now();
  
  try {
    // Parse arguments
    let args: Record<string, unknown>;
    try {
      args = JSON.parse(toolCall.function.arguments || '{}');
    } catch {
      return {
        toolName: toolCall.function.name,
        toolCallId: toolCall.id,
        rawResult: { error: `Invalid JSON arguments: ${toolCall.function.arguments}` },
        timestamp: startTime,
        forQuestionId,
      };
    }
    
    // Execute with timeout
    const timeoutPromise = new Promise<ToolResult>((_, reject) => {
      setTimeout(() => reject(new Error('Tool execution timed out')), TOOL_TIMEOUT_MS);
    });
    
    const result = await Promise.race([
      executeTool(toolCall.function.name, args),
      timeoutPromise,
    ]);
    
    return {
      toolName: toolCall.function.name,
      toolCallId: toolCall.id,
      rawResult: result.success ? result.data : { error: result.error },
      timestamp: startTime,
      forQuestionId,
    };
  } catch (error) {
    // Convert any error to an observation (resilience pattern)
    const errorMessage = error instanceof Error ? error.message : String(error);
    
    console.warn(
      `[runResearchLoop] Tool ${toolCall.function.name} failed:`,
      errorMessage
    );
    
    return {
      toolName: toolCall.function.name,
      toolCallId: toolCall.id,
      rawResult: {
        error: `Failed to execute ${toolCall.function.name}: ${errorMessage}`,
      },
      timestamp: startTime,
      forQuestionId,
    };
  }
}

/**
 * Handle internal research tools (assess_progress, request_synthesis).
 * These are "virtual" tools that don't call external APIs but affect state.
 * 
 * - assess_progress: Logs the agent's strategy update to activity feed
 * - request_synthesis: Validates fact count and triggers phase transition
 */
function handleInternalTool(
  toolCall: ToolCall,
  state: ResearchState
): { observation: PendingObservation; updatedState: ResearchState; shouldTransition?: 'synthesizing' } {
  const toolName = toolCall.function.name;
  let args: Record<string, unknown> = {};
  
  try {
    args = JSON.parse(toolCall.function.arguments || '{}');
  } catch {
    // Use empty args if parse fails
  }
  
  let updatedState = state;
  let rawResult: unknown;
  let shouldTransition: 'synthesizing' | undefined;
  
  if (toolName === 'assess_progress') {
    // === ASSESS_PROGRESS: Log strategy and continue ===
    const claimsCovered = (args.claimsCovered as string[]) || [];
    const remainingGaps = (args.remainingGaps as string[]) || [];
    const strategyUpdate = (args.strategyUpdate as string) || 'Continuing research...';
    
    console.log('[handleInternalTool] assess_progress:', {
      claimsCovered: claimsCovered.length,
      remainingGaps: remainingGaps.length,
      strategyUpdate: strategyUpdate.slice(0, 100),
    });
    
    // Log to activity feed for UI visibility
    updatedState = pushActivityLog(updatedState, `📊 Progress: ${claimsCovered.length} claims covered, ${remainingGaps.length} gaps remaining`);
    if (strategyUpdate && strategyUpdate.length > 0) {
      updatedState = pushActivityLog(updatedState, `🔄 Strategy: ${strategyUpdate.slice(0, 100)}`);
    }
    
    // Update knowledge gaps with remaining gaps (deduplicated)
    if (remainingGaps.length > 0) {
      const existingGapsLower = new Set(updatedState.knowledgeGaps.map(g => g.toLowerCase().trim()));
      const newGaps = remainingGaps.filter(g => !existingGapsLower.has(g.toLowerCase().trim()));
      if (newGaps.length > 0) {
        updatedState = {
          ...updatedState,
          knowledgeGaps: [...updatedState.knowledgeGaps, ...newGaps].slice(0, 10),
        };
      }
    }
    
    rawResult = {
      success: true,
      message: `Progress assessment recorded. ${claimsCovered.length} claims covered, ${remainingGaps.length} gaps identified. Continue researching to fill gaps.`,
    };
    
  } else if (toolName === 'request_synthesis') {
    // === REQUEST_SYNTHESIS: Validate and potentially transition ===
    const reason = (args.reason as string) || 'No reason provided';
    const factCount = updatedState.gatheredFacts.length;
    
    console.log('[handleInternalTool] request_synthesis:', {
      factCount,
      minRequired: MIN_FACTS_FOR_SYNTHESIS,
      reason: reason.slice(0, 100),
    });
    
    if (factCount < MIN_FACTS_FOR_SYNTHESIS) {
      // === REJECT: Insufficient evidence ===
      updatedState = pushActivityLog(
        updatedState,
        `❌ Synthesis REJECTED: Only ${factCount} facts (need ${MIN_FACTS_FOR_SYNTHESIS}+). Keep researching!`
      );
      
      rawResult = {
        success: false,
        rejected: true,
        error: `REJECTED: You have only ${factCount} facts. Minimum ${MIN_FACTS_FOR_SYNTHESIS} facts required for synthesis. Review your searchHistory and try different queries to gather more evidence.`,
        currentFactCount: factCount,
        requiredFactCount: MIN_FACTS_FOR_SYNTHESIS,
        suggestion: 'Try more specific searches, use different keywords, or explore related aspects of the query.',
      };
      
    } else {
      // === ACCEPT: Proceed to synthesis ===
      updatedState = pushActivityLog(
        updatedState,
        `✅ Synthesis approved (${factCount} facts): ${reason.slice(0, 80)}`
      );
      
      // Signal that we should transition to evaluating/synthesizing
      shouldTransition = 'synthesizing';
      
      rawResult = {
        success: true,
        approved: true,
        message: `Synthesis request approved with ${factCount} facts. Transitioning to synthesis phase.`,
        factCount,
        reason,
      };
    }
    
  } else {
    // Unknown internal tool
    rawResult = { error: `Unknown internal tool: ${toolName}` };
  }
  
  const observation: PendingObservation = {
    toolName,
    toolCallId: toolCall.id,
    rawResult,
    timestamp: Date.now(),
  };
  
  return { observation, updatedState, shouldTransition };
}

/**
 * Execute multiple tool calls in parallel with batching and deduplication.
 * All results become observations - failures don't crash the loop.
 * 
 * Deduplication: Search queries are checked against searchHistory using
 * Jaccard similarity. Duplicates return a mock "skip" result instead of
 * executing, preventing redundant API calls across rounds.
 * 
 * Tracking: After execution, successful searches are recorded in searchHistory
 * for future deduplication checks.
 * 
 * Internal Tools: assess_progress and request_synthesis are handled locally
 * without external API calls. request_synthesis can trigger phase transition.
 */
async function executeToolsBatch(
  toolCalls: ToolCall[],
  executeTool: ToolExecutor,
  state: ResearchState,
  forQuestionId?: string
): Promise<{ observations: PendingObservation[]; updatedState: ResearchState; shouldTransition?: 'synthesizing' }> {
  if (toolCalls.length === 0) {
    return { observations: [], updatedState: state };
  }
  
  let updatedState = state;
  let shouldTransition: 'synthesizing' | undefined;
  
  // Batch to prevent overwhelming the system
  const batches: ToolCall[][] = [];
  for (let i = 0; i < toolCalls.length; i += MAX_PARALLEL_TOOLS) {
    batches.push(toolCalls.slice(i, i + MAX_PARALLEL_TOOLS));
  }
  
  const allObservations: PendingObservation[] = [];
  
  for (const batch of batches) {
    // Pre-filter: handle internal tools and check for duplicate searches
    const toolsToExecute: ToolCall[] = [];
    const skippedObservations: PendingObservation[] = [];
    const internalToolResults: PendingObservation[] = [];
    
    for (const tc of batch) {
      const toolName = tc.function.name;
      
      // === HANDLE INTERNAL RESEARCH TOOLS ===
      if (isInternalResearchTool(toolName)) {
        const result = handleInternalTool(tc, updatedState);
        internalToolResults.push(result.observation);
        updatedState = result.updatedState;
        // Capture transition signal from request_synthesis
        if (result.shouldTransition) {
          shouldTransition = result.shouldTransition;
        }
        continue;
      }
      
      // Extract search query from arguments
      let searchQuery: string | undefined;
      try {
        const args = JSON.parse(tc.function.arguments || '{}');
        searchQuery = args.query || args.q || args.search_query || args.search;
      } catch {
        // Ignore parse errors - non-search tools won't have query
      }
      
      // Check for duplicate if this is a search tool with a query
      if (searchQuery) {
        const { isDuplicate, existingRecord } = isSearchDuplicate(
          searchQuery,
          updatedState.searchHistory
        );
        
        if (isDuplicate) {
          console.log(
            `[executeToolsBatch] Skipping duplicate search: "${searchQuery.slice(0, 50)}..." ` +
            `(similar to round ${existingRecord?.round} query)`
          );
          
          // Return a mock observation explaining the skip
          skippedObservations.push({
            toolName: tc.function.name,
            toolCallId: tc.id,
            rawResult: {
              skipped: true,
              reason: 'Search skipped: Query similar to previous search in history. Try a different angle or rephrase your query.',
              similarTo: existingRecord?.query?.slice(0, 100),
              previousRound: existingRecord?.round,
            },
            timestamp: Date.now(),
            forQuestionId,
          });
          
          // Add to knowledge gaps so LLM knows to try different approach
          updatedState = {
            ...updatedState,
            knowledgeGaps: [
              ...updatedState.knowledgeGaps,
              `Search already attempted: "${searchQuery.slice(0, 50)}..." - try different keywords`,
            ].slice(0, 10),
          };
          
          continue;
        }
      }
      
      toolsToExecute.push(tc);
    }
    
    // Add internal tool results first (they're processed synchronously)
    allObservations.push(...internalToolResults);
    
    // Add skipped observations
    allObservations.push(...skippedObservations);
    
    // Execute non-duplicate tools in parallel
    if (toolsToExecute.length > 0) {
      const observations = await Promise.all(
        toolsToExecute.map((tc) => executeToolSafely(tc, executeTool, forQuestionId))
      );
      
      // Track successful searches in history
      for (let i = 0; i < toolsToExecute.length; i++) {
        const tc = toolsToExecute[i];
        
        // Extract search query
        let searchQuery: string | undefined;
        try {
          const args = JSON.parse(tc.function.arguments || '{}');
          searchQuery = args.query || args.q || args.search_query || args.search;
        } catch {
          // Ignore
        }
        
        // Record search if it has a query (regardless of success - we tried it)
        if (searchQuery) {
          updatedState = addSearchRecord(updatedState, {
            query: searchQuery,
            toolName: tc.function.name,
            forQuestionId,
            factIdsProduced: [], // Will be linked later after fact extraction
          });
        }
      }
      
      allObservations.push(...observations);
    }
  }
  
  return { observations: allObservations, updatedState, shouldTransition };
}

// =============================================================================
// Phase Handlers
// =============================================================================

/**
 * Handle the planning phase - decompose query into sub-questions.
 */
async function handlePlanningPhase(
  state: ResearchState,
  llmResponse: LLMResponse
): Promise<ResearchState> {
  console.debug('[runResearchLoop] Planning phase, response length:', llmResponse.content?.length);
  
  const parsed = tryParseStructuredResponse(llmResponse.content);
  console.debug('[runResearchLoop] Parsed plan:', parsed?.type, parsed && 'questions' in parsed ? parsed.questions?.length : 0);
  
  // Log complexity/perspectives if present
  if (parsed && parsed.type === 'plan') {
    console.log('[runResearchLoop] Planning parsed:', {
      complexity: parsed.complexity ?? 'not specified',
      perspectives: parsed.perspectives ?? [],
      questions: parsed.questions?.length ?? 0,
      hypothesis: parsed.hypothesis?.slice(0, 80) + '...',
    });
  }
  
  if (!parsed || parsed.type !== 'plan') {
    // Model didn't follow protocol - try to extract anything useful
    console.warn('[runResearchLoop] Planning phase: invalid response format, creating default plan');
    
    // Create a default question from the original query
    const defaultQuestion = createQuestion(
      `Research: ${state.originalQuery}`,
      1
    );
    
    return {
      ...state,
      researchPlan: [defaultQuestion],
      currentHypothesis: 'No initial hypothesis formed.',
      knowledgeGaps: ['Need to search for information'],
      phase: 'gathering',
    };
  }
  
  // Create questions from plan
  const questions: ResearchQuestion[] = parsed.questions.map((q, idx) =>
    createQuestion(q.question, q.priority ?? idx + 1)
  );
  
  // Parse complexity classification (default to 'simple' if not provided)
  const validComplexities = ['simple', 'multi-faceted', 'controversial'] as const;
  const complexity = parsed.complexity && validComplexities.includes(parsed.complexity as typeof validComplexities[number])
    ? (parsed.complexity as 'simple' | 'multi-faceted' | 'controversial')
    : 'simple';
  
  // Parse perspectives (only meaningful for non-simple queries)
  const perspectives: string[] = Array.isArray(parsed.perspectives)
    ? parsed.perspectives.filter((p: unknown) => typeof p === 'string' && p.trim().length > 0)
    : [];
  
  // Set initial perspective (first one if available, undefined for simple queries)
  const currentPerspective = complexity !== 'simple' && perspectives.length > 0
    ? perspectives[0]
    : undefined;
  
  // Adjust maxRounds to cover all perspectives (minimum 3 rounds)
  // This ensures each perspective gets at least one round of focused research
  const adjustedMaxRounds = perspectives.length > 0
    ? Math.max(perspectives.length, state.maxRounds)
    : state.maxRounds;
  
  // Log planning completion for user visibility
  let newState: ResearchState = {
    ...state,
    researchPlan: questions,
    currentHypothesis: parsed.hypothesis,
    knowledgeGaps: parsed.gaps ?? [],
    complexity,
    perspectives,
    currentPerspective,
    maxRounds: adjustedMaxRounds,
    phase: 'gathering',
  };
  
  // Build activity log message
  const complexityNote = complexity !== 'simple' && perspectives.length > 0
    ? ` (${complexity}: ${perspectives.length} perspectives)`
    : '';
  newState = pushActivityLog(newState, `Created research plan with ${questions.length} questions${complexityNote}`);
  
  return newState;
}

/**
 * Handle the gathering phase - process search results or answers.
 */
async function handleGatheringPhase(
  state: ResearchState,
  llmResponse: LLMResponse,
  observations: PendingObservation[]
): Promise<ResearchState> {
  let newState = { ...state };
  
  // Add observations from tool calls
  if (observations.length > 0) {
    for (const obs of observations) {
      newState = addObservation(newState, obs);
    }
    // Store reasoning that led to the tool calls
    if (llmResponse.content) {
      newState.lastReasoning = llmResponse.content.slice(0, 1500);
    }
    return newState;
  }
  
  // No tool calls - check for structured answer
  const parsed = tryParseStructuredResponse(llmResponse.content);
  
  if (parsed && parsed.type === 'answer') {
    // Resolve the question being answered using multiple fallback strategies
    let targetQuestion: ResearchQuestion | undefined;
    
    // Strategy 1: Use questionIndex (preferred, 1-based)
    if (parsed.questionIndex !== undefined && parsed.questionIndex > 0) {
      targetQuestion = newState.researchPlan[parsed.questionIndex - 1];
    }
    
    // Strategy 2: Fall back to questionId (legacy UUID)
    if (!targetQuestion && parsed.questionId) {
      targetQuestion = newState.researchPlan.find(q => q.id === parsed.questionId);
    }
    
    // Strategy 3: Fall back to current in-progress question
    if (!targetQuestion) {
      targetQuestion = newState.researchPlan.find(q => q.status === 'in-progress');
      if (targetQuestion) {
        console.log(
          `[handleGatheringPhase] Using in-progress question fallback: Q${newState.researchPlan.indexOf(targetQuestion) + 1}`
        );
      }
    }
    
    // If still no match, log warning but continue (facts still get extracted)
    if (!targetQuestion) {
      console.warn(
        '[handleGatheringPhase] Could not resolve target question for answer. ' +
        `questionIndex=${parsed.questionIndex}, questionId=${parsed.questionId}`
      );
    }
    
    const targetQuestionId = targetQuestion?.id ?? 'unknown';
    
    // Extract facts (with defensive handling for missing facts array)
    const factsArray = Array.isArray(parsed.facts) ? parsed.facts : [];
    const newFacts: GatheredFact[] = factsArray.map((f) =>
      createFact(
        f.claim,
        f.sourceUrl,
        f.sourceTitle,
        f.confidence,
        state.currentStep,
        [targetQuestionId]
      )
    );
    
    // Add facts with pruning
    newState = addFacts(newState, newFacts);
    
    // Log fact extraction for user visibility
    if (newFacts.length > 0) {
      newState = pushActivityLog(newState, `Found ${newFacts.length} new fact${newFacts.length === 1 ? '' : 's'}`);
    }
    
    // Update question status if we found a target
    if (targetQuestion) {
      const questionIndex = newState.researchPlan.indexOf(targetQuestion) + 1;
      newState = updateQuestion(newState, targetQuestion.id, {
        status: 'answered',
        answerSummary: parsed.answer.slice(0, 500),
        supportingFactIds: newFacts.map((f) => f.id),
      });
      
      // Log question completion
      const truncatedQuestion = targetQuestion.question.length > 40
        ? targetQuestion.question.slice(0, 37) + '...'
        : targetQuestion.question;
      newState = pushActivityLog(newState, `Answered Q${questionIndex}: "${truncatedQuestion}"`);
    }
    
    // Update hypothesis if provided
    if (parsed.updatedHypothesis) {
      newState.currentHypothesis = parsed.updatedHypothesis;
    }
    
    // Add new gaps if provided (deduplicate by normalizing and comparing)
    if (parsed.newGaps && parsed.newGaps.length > 0) {
      const existingGapsLower = new Set(
        newState.knowledgeGaps.map((g) => g.toLowerCase().trim())
      );
      const uniqueNewGaps = parsed.newGaps.filter(
        (gap) => !existingGapsLower.has(gap.toLowerCase().trim())
      );
      newState.knowledgeGaps = [
        ...newState.knowledgeGaps,
        ...uniqueNewGaps,
      ].slice(0, 10); // Keep max 10 gaps
    }
    
    // Clear observations after processing
    newState = clearObservations(newState);
    
    // Note: Transition to evaluating phase is now handled in the main loop
    // after this function returns, to ensure proper round-aware evaluation
  }
  
  return newState;
}

/**
 * Handle the synthesis phase - generate final report.
 */
async function handleSynthesisPhase(
  state: ResearchState,
  llmResponse: LLMResponse
): Promise<ResearchState> {
  const parsed = tryParseStructuredResponse(llmResponse.content);
  
  if (!parsed || parsed.type !== 'report') {
    // Model didn't follow protocol - use raw content as report
    console.warn('[runResearchLoop] Synthesis phase: invalid response format');
    
    return completeResearch(
      state,
      llmResponse.content || 'Research completed but no report generated.',
      []
    );
  }
  
  // Validate citations reference real facts
  const factIds = new Set(state.gatheredFacts.map((f) => f.id));
  const validCitations = parsed.citations.filter((c) => factIds.has(c.factId));
  
  return completeResearch(state, parsed.report, validCitations);
}

/**
 * Handle the evaluation phase - assess research quality and decide next steps.
 * 
 * This is the key phase for iterative research:
 * - If adequacyScore >= 7 OR no more rounds allowed: transition to SYNTHESIZING
 * - If adequacyScore < 7 AND more rounds allowed: transition to COMPRESSING
 * 
 * The suggestedFollowups are stored in state for the COMPRESSING phase to use.
 */
async function handleEvaluatingPhase(
  state: ResearchState,
  llmResponse: LLMResponse
): Promise<ResearchState> {
  console.debug('[runResearchLoop] Evaluation phase, response length:', llmResponse.content?.length);
  
  const parsed = tryParseStructuredResponse(llmResponse.content);
  
  if (!parsed || parsed.type !== 'evaluation') {
    // Model didn't follow protocol - default to synthesis (conservative)
    console.warn('[runResearchLoop] Evaluation phase: invalid response format, defaulting to synthesis');
    
    let newState = pushActivityLog(state, 'Evaluation unclear, proceeding to synthesis...');
    newState = setPhase(newState, 'synthesizing');
    return newState;
  }
  
  const { adequacyScore, missingAspects, suggestedFollowups, shouldContinue } = parsed;
  
  console.log(
    `[runResearchLoop] Evaluation: score=${adequacyScore}/10, shouldContinue=${shouldContinue}, ` +
    `followups=${suggestedFollowups?.length ?? 0}, canContinue=${canContinueResearch(state)}`
  );
  
  // Log evaluation for user visibility
  let newState = pushActivityLog(
    state,
    `Research quality: ${adequacyScore}/10${missingAspects?.length ? ` (${missingAspects.length} gaps)` : ''}`
  );
  
  // Store missing aspects as knowledge gaps
  if (missingAspects && missingAspects.length > 0) {
    const existingGapsLower = new Set(newState.knowledgeGaps.map(g => g.toLowerCase().trim()));
    const uniqueNewGaps = missingAspects.filter(
      gap => !existingGapsLower.has(gap.toLowerCase().trim())
    );
    newState = {
      ...newState,
      knowledgeGaps: [...newState.knowledgeGaps, ...uniqueNewGaps].slice(0, 15),
    };
  }
  
  // Store follow-ups for COMPRESSING phase to convert into questions
  // We'll store them in a temporary field that gets cleared after use
  (newState as ResearchState & { _pendingFollowups?: EvaluationResponse['suggestedFollowups'] })._pendingFollowups = suggestedFollowups;
  
  // === Intelligent Synthesis Decision Logic ===
  // Adapts based on complexity, perspective coverage, and fact diversity
  const synthesisDecision = calculateSynthesisReadiness(newState, adequacyScore, shouldContinue);
  
  console.log('[runResearchLoop] Synthesis decision:', {
    rawScore: adequacyScore,
    adjustedScore: synthesisDecision.adjustedScore,
    threshold: synthesisDecision.threshold,
    perspectiveCoverage: synthesisDecision.perspectiveCoverage,
    factDiversity: synthesisDecision.factDiversity,
    shouldSynthesize: synthesisDecision.shouldSynthesize,
    reason: synthesisDecision.reason,
  });
  
  if (synthesisDecision.shouldSynthesize) {
    console.log(`[runResearchLoop] Evaluation → Synthesis: ${synthesisDecision.reason}`);
    newState = pushActivityLog(newState, `${synthesisDecision.reason}, synthesizing...`);
    newState = setPhase(newState, 'synthesizing');
  } else {
    console.log(
      `[runResearchLoop] Evaluation → Compressing: ${synthesisDecision.reason}, ` +
      `round ${newState.currentRound}/${newState.maxRounds}`
    );
    newState = pushActivityLog(
      newState,
      `${synthesisDecision.reason}, starting round ${newState.currentRound + 1}...`
    );
    newState = setPhase(newState, 'compressing');
  }
  
  return newState;
}

/**
 * Handle the compression phase - summarize current round and prepare for next.
 * 
 * This phase:
 * 1. Generates a compressed summary of the current round's findings
 * 2. Converts suggested follow-ups into new ResearchQuestion entries
 * 3. Advances to the next round
 * 4. Transitions back to GATHERING
 */
async function handleCompressingPhase(
  state: ResearchState,
  llmResponse: LLMResponse
): Promise<ResearchState> {
  console.debug('[runResearchLoop] Compression phase, response length:', llmResponse.content?.length);
  
  const parsed = tryParseStructuredResponse(llmResponse.content);
  
  let summary: string;
  if (parsed && parsed.type === 'roundSummary') {
    summary = parsed.summary;
    console.log(`[runResearchLoop] Round summary generated: ${summary.length} chars, ${parsed.keyInsights?.length ?? 0} insights`);
  } else {
    // Fallback: use raw content as summary
    console.warn('[runResearchLoop] Compression phase: invalid response format, using raw content');
    summary = llmResponse.content?.slice(0, 500) || `Round ${state.currentRound} findings summarized.`;
  }
  
  // Create the round summary (this captures current fact IDs for dual-layer context)
  // Also record the perspective this round was researching
  let newState = createRoundSummary(state, summary, state.currentPerspective);
  newState = pushActivityLog(newState, `Round ${newState.currentRound} compressed`);
  
  // Convert pending follow-ups into new research questions
  const pendingFollowups = (state as ResearchState & { _pendingFollowups?: EvaluationResponse['suggestedFollowups'] })._pendingFollowups;
  
  if (pendingFollowups && pendingFollowups.length > 0) {
    // Filter to avoid duplicate questions
    const existingQuestions = new Set(
      newState.researchPlan.map(q => q.question.toLowerCase().trim())
    );
    
    const newQuestions: ResearchQuestion[] = [];
    for (const followup of pendingFollowups) {
      const normalizedQuestion = followup.question.toLowerCase().trim();
      if (!existingQuestions.has(normalizedQuestion)) {
        newQuestions.push(createQuestion(followup.question, followup.priority));
        existingQuestions.add(normalizedQuestion);
      }
    }
    
    if (newQuestions.length > 0) {
      console.log(`[runResearchLoop] Adding ${newQuestions.length} follow-up questions for round ${newState.currentRound + 1}`);
      newState = {
        ...newState,
        researchPlan: [...newState.researchPlan, ...newQuestions],
      };
      newState = pushActivityLog(newState, `Added ${newQuestions.length} follow-up questions`);
    }
  }
  
  // Clear the temporary follow-ups storage
  delete (newState as ResearchState & { _pendingFollowups?: unknown })._pendingFollowups;
  
  // Advance to next round
  newState = advanceRound(newState);
  console.log(`[runResearchLoop] Advanced to round ${newState.currentRound}/${newState.maxRounds}`, {
    perspective: newState.currentPerspective ?? 'none',
    complexity: newState.complexity ?? 'simple',
  });
  
  // Transition back to gathering
  newState = setPhase(newState, 'gathering');
  
  // Build descriptive activity log message
  const perspectiveNote = newState.currentPerspective
    ? ` (Perspective: ${newState.currentPerspective})`
    : '';
  newState = pushActivityLog(newState, `Starting round ${newState.currentRound}${perspectiveNote}...`);
  
  return newState;
}

// =============================================================================
// Soft Landing Logic (Round-Aware)
// =============================================================================

/**
 * Check if we should force a phase transition (soft landing guardrail).
 * 
 * Now uses round-based budgets (60/30/10 split) instead of global step count.
 * When round budget is exhausted:
 * - If gathering: transition to EVALUATING (not directly to synthesis)
 * - If already evaluating/compressing: let those phases complete naturally
 */
function shouldForceEvaluation(state: ResearchState): boolean {
  // Only applies during gathering phase
  if (state.phase !== 'gathering') {
    return false;
  }
  
  return shouldTriggerRoundSoftLanding(state);
}

/**
 * Check if we've hit the hard global limit and should force immediate synthesis.
 * This is a fallback when round-based soft landing isn't enough.
 */
function shouldForceImmediateSynthesis(state: ResearchState): boolean {
  const threshold = Math.floor(state.maxSteps * SOFT_LANDING_THRESHOLD);
  return state.currentStep >= threshold && 
    (state.phase === 'gathering' || state.phase === 'evaluating' || state.phase === 'compressing');
}

/**
 * Get soft landing instruction to append to phase instruction.
 */
function getSoftLandingInstruction(state: ResearchState): string {
  const remaining = state.maxSteps - state.currentStep;
  const { stepsRemainingThisRound } = getRoundStepBudget(state);
  
  return `
⚠️ TIME CONSTRAINT: Only ${remaining} steps remaining (${stepsRemainingThisRound} in current round).
INSTRUCTION: Stop searching immediately. You must synthesize your findings NOW.
Use the facts and partial answers you have gathered. Do not request more searches.
Output a final report with what you know, noting any gaps as limitations.`;
}

/**
 * Get evaluation urgency instruction when round budget is low.
 */
function getRoundBudgetWarning(state: ResearchState): string {
  const { stepsRemainingThisRound, roundBudget } = getRoundStepBudget(state);
  
  return `
📊 ROUND BUDGET: ${stepsRemainingThisRound}/${roundBudget} steps remaining in round ${state.currentRound}.
Consider wrapping up current questions before the round budget expires.`;
}

// =============================================================================
// Human-in-the-Loop Intervention
// =============================================================================

/**
 * Handle a user intervention signal.
 * 
 * Supports two intervention types:
 * - 'wrap-up': Force immediate synthesis with what we have
 * - 'skip-question': Mark a question as blocked and move on
 * 
 * Skip behavior differs based on whether the target is the current focus:
 * - Current focus: Use timeout logic (mark blocked, log gap, transition to next)
 * - Pending question: Simply flip status to blocked (no disruption to current flow)
 */
function handleIntervention(
  state: ResearchState,
  intervention: ResearchIntervention
): ResearchState {
  switch (intervention.type) {
    case 'wrap-up': {
      console.log('[runResearchLoop] User requested wrap-up, forcing synthesis');
      
      // Set manual termination flag and transition to synthesizing
      return {
        ...state,
        phase: 'synthesizing',
        isManualTermination: true,
      };
    }
    
    case 'skip-question': {
      const { questionId } = intervention;
      const targetQuestion = state.researchPlan.find(q => q.id === questionId);
      
      if (!targetQuestion) {
        console.warn(`[runResearchLoop] Skip intervention: question ${questionId} not found`);
        return state;
      }
      
      // Already answered or blocked? No-op
      if (targetQuestion.status === 'answered' || targetQuestion.status === 'blocked') {
        console.log(`[runResearchLoop] Skip intervention: question already ${targetQuestion.status}`);
        return state;
      }
      
      const questionIndex = state.researchPlan.indexOf(targetQuestion) + 1;
      const isCurrentFocus = targetQuestion.status === 'in-progress';
      
      console.log(
        `[runResearchLoop] User skipped Q${questionIndex}${isCurrentFocus ? ' (current focus)' : ' (pending)'}`
      );
      
      // Mark the question as blocked
      let newState = {
        ...state,
        researchPlan: state.researchPlan.map(q =>
          q.id === questionId
            ? { ...q, status: 'blocked' as const }
            : q
        ),
      };
      
      // If it was the current focus, also add to knowledge gaps
      // (mirrors the timeout logic from Focus Timeout feature)
      if (isCurrentFocus) {
        newState = {
          ...newState,
          knowledgeGaps: [
            ...newState.knowledgeGaps,
            `Skipped by user: Q${questionIndex}: "${targetQuestion.question}"`,
          ],
        };
      }
      
      return newState;
    }
    
    default:
      console.warn('[runResearchLoop] Unknown intervention type:', intervention);
      return state;
  }
}

// =============================================================================
// Main Loop
// =============================================================================

/**
 * Run the deep research loop.
 *
 * Implements the Plan-and-Execute state machine with multi-round support:
 * PLANNING → GATHERING → EVALUATING → [COMPRESSING → GATHERING]* → SYNTHESIZING → COMPLETE
 *
 * Multi-round iteration:
 * - After gathering completes, EVALUATING phase assesses research quality (1-10 score)
 * - If score < 7 and rounds remain: COMPRESSING summarizes, adds follow-up questions, loops back
 * - If score >= 7 or max rounds reached: proceeds to SYNTHESIZING
 *
 * With stability patterns:
 * 1. Round-based soft landing (60/30/10 budget split)
 * 2. Query deduplication across rounds
 * 3. Parallel batch tool execution
 * 4. Tool failure resilience
 * 5. Dual-layer context (compressed summaries for prompts, full facts for synthesis)
 */
export async function runResearchLoop(
  options: RunResearchLoopOptions
): Promise<ResearchLoopResult> {
  const {
    query,
    messageId,
    conversationId,
    modelRouting,
    baseSystemPrompt,
    tools,
    executeTool,
    callLLM,
    maxSteps = DEFAULT_MAX_STEPS,
    onStateUpdate,
    onStatePersist,
    abortSignal,
    maxContextTokens = 8000,
    interventionRef,
  } = options;

  // Initialize state
  let state = createInitialState(query, messageId, {
    conversationId,
    maxSteps,
  });

  // Notify UI of initial state
  onStateUpdate?.(state);

  console.log('[runResearchLoop] Starting research:', {
    query: query.slice(0, 100),
    maxSteps,
    tools: tools.length,
  });

  // Get research tools including internal agentic tools (assess_progress, request_synthesis)
  const researchTools = getResearchToolsWithInternals(tools);
  console.log('[runResearchLoop] Research tools:', researchTools.map(t => t.function.name));

  try {
    // === MAIN LOOP ===
    while (state.phase !== 'complete' && state.phase !== 'error') {
      // Check for cancellation
      if (abortSignal?.aborted) {
        state = setError(state, 'Research cancelled by user');
        break;
      }

      // === HUMAN-IN-THE-LOOP INTERVENTION CHECK ===
      // Read intervention signal from ref (written by UI)
      const intervention = interventionRef?.current;
      if (intervention) {
        // Clear the intervention to prevent re-processing
        interventionRef.current = null;
        
        // Log the intervention for user visibility
        const logMessage = intervention.type === 'wrap-up'
          ? 'User intervention: Wrapping up...'
          : 'User intervention: Skipping question...';
        state = pushActivityLog(state, logMessage);
        
        state = handleIntervention(state, intervention);
        
        console.log('[runResearchLoop] Processed intervention:', intervention.type, 'new phase:', state.phase);
        
        // Notify UI of intervention-caused state change
        onStateUpdate?.(state);
        
        // Re-evaluate loop with new state immediately - don't proceed with current iteration
        // This ensures wrap-up triggers synthesis and skip properly transitions to next question
        continue;
      }

      // Advance step counter
      state = advanceStep(state);

      // Hard limit check
      if (state.currentStep > state.maxSteps) {
        console.warn('[runResearchLoop] Hard step limit reached');
        state = setError(
          state,
          `Maximum steps (${state.maxSteps}) reached without completing research.`
        );
        break;
      }

      // === FOCUS TIMEOUT CHECK ===
      // Check if any question has been in-progress too long and should be marked blocked
      if (state.phase === 'gathering') {
        const timedOutQuestion = state.researchPlan.find(
          (q) =>
            q.status === 'in-progress' &&
            q.inProgressSince !== undefined &&
            state.currentStep - q.inProgressSince >= QUESTION_FOCUS_TIMEOUT_STEPS
        );

        if (timedOutQuestion) {
          const questionIndex = state.researchPlan.indexOf(timedOutQuestion) + 1;
          console.log(
            `[runResearchLoop] Question Q${questionIndex} timed out after ${QUESTION_FOCUS_TIMEOUT_STEPS} steps, marking as blocked`
          );

          // Mark as blocked and record knowledge gap
          state = {
            ...state,
            researchPlan: state.researchPlan.map((q) =>
              q.id === timedOutQuestion.id
                ? { ...q, status: 'blocked' as const }
                : q
            ),
            knowledgeGaps: [
              ...state.knowledgeGaps,
              `Unable to find definitive data for Q${questionIndex}: "${timedOutQuestion.question}" after ${QUESTION_FOCUS_TIMEOUT_STEPS} attempts`,
            ],
          };
          
          // Log the timeout for user visibility
          state = pushActivityLog(state, `Q${questionIndex} timed out, moving on...`);
        }
      }

      // === ENSURE IN-PROGRESS QUESTION ===
      // If in gathering phase and no question is in-progress, mark the next pending one
      if (state.phase === 'gathering') {
        const hasInProgress = state.researchPlan.some(
          (q) => q.status === 'in-progress'
        );

        if (!hasInProgress) {
          const nextPending = state.researchPlan.find(
            (q) => q.status === 'pending'
          );

          if (nextPending) {
            const questionIndex = state.researchPlan.indexOf(nextPending) + 1;
            console.log(
              `[runResearchLoop] Setting Q${questionIndex} as current focus`
            );
            
            // Log question transition for user visibility
            const truncatedQuestion = nextPending.question.length > 40
              ? nextPending.question.slice(0, 37) + '...'
              : nextPending.question;
            state = pushActivityLog(state, `Moving to Q${questionIndex}: "${truncatedQuestion}"`);

            state = {
              ...state,
              researchPlan: state.researchPlan.map((q) =>
                q.id === nextPending.id
                  ? { ...q, status: 'in-progress' as const, inProgressSince: state.currentStep }
                  : q
              ),
            };
          }
        }
      }

      // === SOFT LANDING GUARDRAILS (Round-Aware) ===
      let phaseInstruction: string | undefined;
      
      // Check for global hard limit approaching - force immediate synthesis
      if (shouldForceImmediateSynthesis(state)) {
        console.log('[runResearchLoop] Global soft landing triggered - forcing immediate synthesis');
        state = setPhase(state, 'synthesizing');
        state = pushActivityLog(state, 'Time limit approaching, synthesizing...');
        phaseInstruction =
          PHASE_INSTRUCTIONS.synthesizing + getSoftLandingInstruction(state);
      }
      // Check for round budget exhaustion - trigger evaluation (not direct synthesis)
      else if (shouldForceEvaluation(state)) {
        const { stepsUsedThisRound, roundBudget } = getRoundStepBudget(state);
        console.log(
          `[runResearchLoop] Round soft landing triggered - round ${state.currentRound} budget ` +
          `(${stepsUsedThisRound}/${roundBudget}) at 80%, moving to evaluation`
        );
        
        // Check if all questions are answered - if so, go to evaluation
        const unanswered = state.researchPlan.filter(
          (q) => q.status !== 'answered' && q.status !== 'blocked'
        );
        
        if (unanswered.length === 0) {
          // All questions answered, go to evaluation
          state = setPhase(state, 'evaluating');
          state = pushActivityLog(state, 'Round complete, evaluating research quality...');
        } else {
          // Still have unanswered questions but budget is low
          // Mark remaining as blocked and move to evaluation
          console.log(`[runResearchLoop] ${unanswered.length} questions remaining, marking as blocked due to budget`);
          
          for (const q of unanswered) {
            const questionIndex = state.researchPlan.indexOf(q) + 1;
            state = {
              ...state,
              researchPlan: state.researchPlan.map((rq) =>
                rq.id === q.id ? { ...rq, status: 'blocked' as const } : rq
              ),
              knowledgeGaps: [
                ...state.knowledgeGaps,
                `Round ${state.currentRound} budget exhausted: Q${questionIndex}: "${q.question}"`,
              ],
            };
          }
          
          state = setPhase(state, 'evaluating');
          state = pushActivityLog(state, `Round ${state.currentRound} budget exhausted, evaluating...`);
        }
      }
      
      // Add round budget warning to gathering phase instruction if getting low
      if (state.phase === 'gathering' && !phaseInstruction) {
        const { stepsRemainingThisRound, roundBudget } = getRoundStepBudget(state);
        if (stepsRemainingThisRound <= roundBudget * 0.3 && stepsRemainingThisRound > 0) {
          phaseInstruction = PHASE_INSTRUCTIONS.gathering + getRoundBudgetWarning(state);
        }
      }

      // Log phase/step
      console.log(
        `[runResearchLoop] Step ${state.currentStep}/${state.maxSteps} - Phase: ${state.phase}`
      );
      
      // Notify UI of step start
      onStateUpdate?.(state);

      // === BUILD MESSAGES ===
      const turnMessages = buildTurnMessagesWithBudget(
        {
          state,
          baseSystemPrompt,
          phaseInstruction,
        },
        maxContextTokens
      );

      // Determine which model to use
      // - Extraction model (cheap/fast): for fact extraction during gathering, and for compression
      // - Reasoning model (capable): for planning, evaluation, and synthesis
      let endpoint: ModelEndpoint;
      if (state.phase === 'gathering' && state.pendingObservations.length > 0) {
        // Fact extraction from observations
        endpoint = modelRouting.extractionModel;
      } else if (state.phase === 'compressing') {
        // Round summary generation (use extraction or summarization model)
        endpoint = modelRouting.summarizationModel ?? modelRouting.extractionModel;
      } else {
        // Planning, evaluation, synthesis need reasoning
        endpoint = modelRouting.reasoningModel;
      }

      // Determine if tools should be available
      const includeTools = shouldIncludeTools(state.phase);

      // === CALL LLM ===
      // Mark LLM as generating for UI feedback
      state = setLLMGenerating(state, true, 'Thinking...');
      onStateUpdate?.(state);
      
      let llmResponse: LLMResponse;
      try {
        llmResponse = await callLLM(turnMessages.messages, {
          tools: includeTools ? researchTools : undefined,
          endpoint,
          abortSignal,
        });
      } catch (error) {
        const errorMsg = error instanceof Error ? error.message : String(error);
        console.error('[runResearchLoop] LLM call failed:', errorMsg);
        state = setLLMGenerating(state, false);
        state = setError(state, `LLM call failed: ${errorMsg}`);
        break;
      }
      
      // Clear LLM generating state
      state = setLLMGenerating(state, false);

      // === EXECUTE TOOLS (if any) ===
      let observations: PendingObservation[] = [];

      if (llmResponse.toolCalls.length > 0) {
        console.log(
          `[runResearchLoop] Executing ${llmResponse.toolCalls.length} tool(s) in parallel`
        );

        // Find current in-progress question for attribution
        const currentQuestion = state.researchPlan.find(
          (q) => q.status === 'in-progress'
        );

        // Parse tool calls into ActiveToolCall format for UI visibility
        const activeToolCalls: ActiveToolCall[] = llmResponse.toolCalls.map(tc => {
          let searchQuery: string | undefined;
          try {
            const args = JSON.parse(tc.function.arguments || '{}');
            // Common search query argument names
            searchQuery = args.query || args.q || args.search_query || args.search;
          } catch {
            // Ignore parse errors
          }
          return {
            toolName: tc.function.name,
            toolCallId: tc.id,
            searchQuery,
            startedAt: Date.now(),
          };
        });
        
        // Update state with active tools (also logs search queries)
        state = setActiveToolCalls(state, activeToolCalls);
        onStateUpdate?.(state);

        // Execute tools with deduplication and tracking
        const toolResult = await executeToolsBatch(
          llmResponse.toolCalls,
          executeTool,
          state,
          currentQuestion?.id
        );
        
        observations = toolResult.observations;
        state = toolResult.updatedState;
        
        // Check if agent requested synthesis (via request_synthesis tool)
        if (toolResult.shouldTransition === 'synthesizing') {
          console.log('[runResearchLoop] Agent requested synthesis, transitioning...');
          state = setPhase(state, 'synthesizing');
        }

        // Clear active tools after completion
        state = clearActiveToolCalls(state);

        console.log(
          `[runResearchLoop] Tools completed:`,
          observations.map((o) => ({
            tool: o.toolName,
            hasError: 'error' in (o.rawResult as Record<string, unknown>),
            skipped: (o.rawResult as Record<string, unknown>)?.skipped === true,
          }))
        );
      }

      // === PROCESS RESPONSE BY PHASE ===
      switch (state.phase) {
        case 'planning':
          state = await handlePlanningPhase(state, llmResponse);
          break;

        case 'gathering':
          state = await handleGatheringPhase(state, llmResponse, observations);
          
          // Check if all questions answered - transition to evaluating (not directly to synthesis)
          if (state.phase === 'gathering') {
            const unanswered = state.researchPlan.filter(
              (q) => q.status !== 'answered' && q.status !== 'blocked'
            );
            
            if (unanswered.length === 0) {
              console.log('[runResearchLoop] All questions answered, moving to evaluation');
              state = setPhase(state, 'evaluating');
              state = pushActivityLog(state, 'All questions answered, evaluating research quality...');
            }
          }
          break;

        case 'evaluating':
          state = await handleEvaluatingPhase(state, llmResponse);
          break;

        case 'compressing':
          state = await handleCompressingPhase(state, llmResponse);
          break;

        case 'synthesizing':
          state = await handleSynthesisPhase(state, llmResponse);
          break;

        default:
          console.warn(`[runResearchLoop] Unexpected phase: ${state.phase}`);
      }

      // === NOTIFY UI ===
      onStateUpdate?.(state);

      // === PERSIST STATE ===
      if (onStatePersist) {
        try {
          await onStatePersist(state);
        } catch (error) {
          console.error('[runResearchLoop] State persistence failed:', error);
          // Don't fail the loop on persistence errors
        }
      }

      // Small delay to prevent tight loops on fast responses
      await sleep(100);
    }
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    console.error('[runResearchLoop] Unexpected error:', errorMsg);
    state = setError(state, `Unexpected error: ${errorMsg}`);
  }

  // Final state update
  onStateUpdate?.(state);

  // Final persistence
  if (onStatePersist) {
    try {
      await onStatePersist(state);
    } catch (error) {
      console.error('[runResearchLoop] Final persistence failed:', error);
    }
  }

  console.log('[runResearchLoop] Research complete:', {
    phase: state.phase,
    steps: state.currentStep,
    rounds: state.currentRound,
    roundSummaries: state.roundSummaries.length,
    facts: state.gatheredFacts.length,
    questions: state.researchPlan.length,
    searchesExecuted: state.searchHistory.length,
    hasReport: !!state.finalReport,
  });

  return {
    state,
    success: state.phase === 'complete',
    error: state.errorMessage,
  };
}

// =============================================================================
// Utilities
// =============================================================================

/**
 * Sleep for specified milliseconds.
 */
function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Create a simple LLM caller that uses fetch to call the local proxy.
 *
 * This is a reference implementation - real usage may need to integrate
 * with gglib's existing streaming infrastructure.
 */
export function createProxyLLMCaller(baseUrl: string = ''): LLMCaller {
  return async (messages, options) => {
    const { tools, endpoint, abortSignal } = options;

    const response = await fetch(
      `${baseUrl}http://localhost:${endpoint.port}/v1/chat/completions`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          model: 'default',
          messages: messages.map((m) => ({ role: m.role, content: m.content })),
          tools,
          stream: false,
        }),
        signal: abortSignal,
      }
    );

    if (!response.ok) {
      throw new Error(`LLM request failed: ${response.status}`);
    }

    const data = await response.json();
    const choice = data.choices?.[0];

    return {
      content: choice?.message?.content ?? '',
      toolCalls: choice?.message?.tool_calls ?? [],
      finishReason: choice?.finish_reason ?? 'stop',
    };
  };
}

/**
 * Create a tool executor from the gglib tool registry.
 *
 * This bridges the research loop to gglib's existing tool infrastructure.
 */
export function createRegistryToolExecutor(
  registry: {
    executeRawCall: (call: {
      id: string;
      type: string;
      function: { name: string; arguments: string };
    }) => Promise<ToolResult>;
  }
): ToolExecutor {
  return async (name, args) => {
    return registry.executeRawCall({
      id: crypto.randomUUID(),
      type: 'function',
      function: {
        name,
        arguments: JSON.stringify(args),
      },
    });
  };
}
