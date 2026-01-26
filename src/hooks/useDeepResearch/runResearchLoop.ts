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
} from './types';
import {
  buildTurnMessagesWithBudget,
  shouldIncludeTools,
  filterResearchTools,
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

type StructuredResponse = PlanResponse | AnswerResponse | ReportResponse;

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
    
    const validTypes = ['plan', 'answer', 'report'];
    if (!validTypes.includes(parsed.type)) {
      return null;
    }
    
    return parsed as StructuredResponse;
  } catch {
    return null;
  }
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
 * Execute multiple tool calls in parallel with batching.
 * All results become observations - failures don't crash the loop.
 */
async function executeToolsBatch(
  toolCalls: ToolCall[],
  executeTool: ToolExecutor,
  forQuestionId?: string
): Promise<PendingObservation[]> {
  if (toolCalls.length === 0) {
    return [];
  }
  
  // Batch to prevent overwhelming the system
  const batches: ToolCall[][] = [];
  for (let i = 0; i < toolCalls.length; i += MAX_PARALLEL_TOOLS) {
    batches.push(toolCalls.slice(i, i + MAX_PARALLEL_TOOLS));
  }
  
  const allObservations: PendingObservation[] = [];
  
  for (const batch of batches) {
    // Execute batch in parallel
    const observations = await Promise.all(
      batch.map((tc) => executeToolSafely(tc, executeTool, forQuestionId))
    );
    
    allObservations.push(...observations);
  }
  
  return allObservations;
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
  
  return {
    ...state,
    researchPlan: questions,
    currentHypothesis: parsed.hypothesis,
    knowledgeGaps: parsed.gaps ?? [],
    phase: 'gathering',
  };
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
    
    // Extract facts
    const newFacts: GatheredFact[] = parsed.facts.map((f) =>
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
    
    // Update question status if we found a target
    if (targetQuestion) {
      newState = updateQuestion(newState, targetQuestion.id, {
        status: 'answered',
        answerSummary: parsed.answer.slice(0, 500),
        supportingFactIds: newFacts.map((f) => f.id),
      });
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
    
    // Check if all questions answered - move to synthesis
    const unanswered = newState.researchPlan.filter(
      (q) => q.status !== 'answered'
    );
    
    if (unanswered.length === 0) {
      newState.phase = 'synthesizing';
    }
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

// =============================================================================
// Soft Landing Logic
// =============================================================================

/**
 * Check if we should force synthesis (soft landing guardrail).
 */
function shouldForceSynthesis(state: ResearchState): boolean {
  const threshold = Math.floor(state.maxSteps * SOFT_LANDING_THRESHOLD);
  return state.currentStep >= threshold && state.phase === 'gathering';
}

/**
 * Get soft landing instruction to append to phase instruction.
 */
function getSoftLandingInstruction(state: ResearchState): string {
  const remaining = state.maxSteps - state.currentStep;
  
  return `
⚠️ TIME CONSTRAINT: Only ${remaining} steps remaining before hard limit.
INSTRUCTION: Stop searching immediately. You must synthesize your findings NOW.
Use the facts and partial answers you have gathered. Do not request more searches.
Output a final report with what you know, noting any gaps as limitations.`;
}

// =============================================================================
// Main Loop
// =============================================================================

/**
 * Run the deep research loop.
 *
 * Implements the Plan-and-Execute state machine:
 * PLANNING → GATHERING → SYNTHESIZING → COMPLETE
 *
 * With stability patterns:
 * 1. Soft landing at 80% steps
 * 2. Parallel batch tool execution
 * 3. Tool failure resilience
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

  // Filter to research-relevant tools
  const researchTools = filterResearchTools(tools);

  try {
    // === MAIN LOOP ===
    while (state.phase !== 'complete' && state.phase !== 'error') {
      // Check for cancellation
      if (abortSignal?.aborted) {
        state = setError(state, 'Research cancelled by user');
        break;
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
              `Unable to find definitive data for Q${questionIndex}: "${timedOutQuestion.text}" after ${QUESTION_FOCUS_TIMEOUT_STEPS} attempts`,
            ],
          };
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

      // === SOFT LANDING GUARDRAIL ===
      let phaseInstruction: string | undefined;
      
      if (shouldForceSynthesis(state)) {
        console.log('[runResearchLoop] Soft landing triggered - forcing synthesis');
        state = setPhase(state, 'synthesizing');
        phaseInstruction =
          PHASE_INSTRUCTIONS.synthesizing + getSoftLandingInstruction(state);
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
      const endpoint =
        state.phase === 'gathering' && state.pendingObservations.length > 0
          ? modelRouting.extractionModel // Use cheap model for fact extraction
          : modelRouting.reasoningModel; // Use capable model for reasoning

      // Determine if tools should be available
      const includeTools = shouldIncludeTools(state.phase);

      // === CALL LLM ===
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
        state = setError(state, `LLM call failed: ${errorMsg}`);
        break;
      }

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

        // Execute tools with resilience pattern
        observations = await executeToolsBatch(
          llmResponse.toolCalls,
          executeTool,
          currentQuestion?.id
        );

        console.log(
          `[runResearchLoop] Tools completed:`,
          observations.map((o) => ({
            tool: o.toolName,
            hasError: 'error' in (o.rawResult as Record<string, unknown>),
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
    facts: state.gatheredFacts.length,
    questions: state.researchPlan.length,
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
