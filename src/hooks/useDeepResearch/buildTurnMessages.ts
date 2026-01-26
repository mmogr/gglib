/**
 * Turn Message Construction for Deep Research Mode
 *
 * Implements the "State-Only" context pattern where:
 * - Research context is injected into the system prompt (not as messages)
 * - Message history is minimal: [System, User] only
 * - Last assistant reasoning is rendered in system prompt (avoids orphaned tool errors)
 *
 * This approach ensures API compatibility across all LLM providers by never
 * sending tool_call messages without their corresponding tool results.
 *
 * @module useDeepResearch/buildTurnMessages
 */

import type {
  ResearchState,
  ResearchPhase,
  ResearchContextInjection,
} from './types';
import {
  serializeForPrompt,
  renderContextForSystemPrompt,
  DEFAULT_BUDGET,
  type SerializationBudget,
} from './types';

// =============================================================================
// Types
// =============================================================================

/**
 * A single message in the LLM API format.
 * Intentionally simplified - no tool_calls, no tool results.
 */
export interface TurnMessage {
  role: 'system' | 'user' | 'assistant';
  content: string;
}

/**
 * The complete message array ready for LLM API call.
 * Always exactly [SystemMessage, UserMessage] for research mode.
 */
export interface TurnMessages {
  /** The constructed messages (always length 2 in state-only mode) */
  messages: TurnMessage[];
  /** The serialized context for debugging/logging */
  contextInjection: ResearchContextInjection;
  /** Total character count for budget monitoring */
  totalChars: number;
}

/**
 * Options for building turn messages.
 */
export interface BuildTurnMessagesOptions {
  /** The current research state (scratchpad) */
  state: ResearchState;
  /** Base system prompt with safety/personality instructions */
  baseSystemPrompt: string;
  /** Optional: phase-specific instruction override */
  phaseInstruction?: string;
  /** Optional: custom serialization budget */
  budget?: SerializationBudget;
  /** Optional: placeholder in baseSystemPrompt to inject context (default: append) */
  contextPlaceholder?: string;
}

// =============================================================================
// Phase-Specific Instructions
// =============================================================================

/**
 * Default instructions for each research phase.
 * These guide the model's behavior for the current step.
 */
export const PHASE_INSTRUCTIONS: Record<ResearchPhase, string> = {
  planning: `## Current Task: PLANNING

You are decomposing the user's research query into actionable sub-questions.

INSTRUCTIONS:
1. Analyze the query to identify 3-7 distinct sub-questions that need answering
2. Form an initial hypothesis based on your existing knowledge
3. Identify what you DON'T know (knowledge gaps)

RESPOND WITH JSON:
{
  "type": "plan",
  "hypothesis": "Your initial working hypothesis...",
  "questions": [
    {"question": "Sub-question 1?", "priority": 1},
    {"question": "Sub-question 2?", "priority": 2}
  ],
  "gaps": ["What we don't know yet..."]
}

Do NOT use tools in planning phase. Output ONLY the JSON.`,

  gathering: `## Current Task: GATHERING

You are searching for information to answer the research questions.

INSTRUCTIONS:
1. Check the "🎯 Current Focus" section to see which question you are working on
2. Use the search tool to find relevant information for that question
3. When you have enough information, provide an AnswerResponse

IF YOU NEED TO SEARCH:
Call the appropriate search tool (e.g., tavily_search) with a well-crafted query.

IF YOU CAN ANSWER THE CURRENT FOCUS QUESTION:
Respond with JSON:
{
  "type": "answer",
  "questionIndex": 1,
  "answer": "Brief answer summary (max 500 chars)",
  "facts": [
    {
      "claim": "Specific factual claim",
      "sourceUrl": "https://...",
      "sourceTitle": "Source Name",
      "confidence": "high|medium|low"
    }
  ],
  "updatedHypothesis": "Refined hypothesis based on new info...",
  "newGaps": ["Newly discovered unknowns..."]
}

IMPORTANT:
- "questionIndex" is the Q number from the Research Plan (Q1, Q2, etc.)
- Check the "🎯 Current Focus" section for the exact questionIndex to use
- Choose ONE action: either call a tool OR output JSON. Not both.`,

  synthesizing: `## Current Task: SYNTHESIZING

You are producing the final research report from gathered facts.

INSTRUCTIONS:
1. Review all gathered facts and answered questions
2. Resolve any contradictions by weighing source credibility
3. Write a comprehensive report with proper citations

RESPOND WITH JSON:
{
  "type": "report",
  "report": "# Research Report\\n\\nYour detailed findings with [1] citation markers...",
  "citations": [
    {"factId": "fact-uuid", "footnoteNumber": 1}
  ],
  "confidence": "high|medium|low",
  "limitations": ["What this research couldn't determine..."]
}

Do NOT use tools in synthesis phase. Output ONLY the JSON.`,

  complete: `Research is complete. No further action needed.`,

  error: `Research encountered an error. Analyze the error and suggest recovery.`,
};

// =============================================================================
// System Prompt Construction
// =============================================================================

/**
 * Build the complete system prompt with research context injected.
 *
 * Composition order:
 * 1. Base system prompt (safety/personality)
 * 2. Research context (state, facts, plan)
 * 3. Phase-specific instruction
 *
 * If `contextPlaceholder` is provided and found in baseSystemPrompt,
 * the context is injected at that location. Otherwise, it's appended.
 */
function buildSystemPrompt(
  baseSystemPrompt: string,
  contextString: string,
  phaseInstruction: string,
  contextPlaceholder?: string
): string {
  const sections: string[] = [];

  // 1. Base prompt with optional placeholder injection
  if (contextPlaceholder && baseSystemPrompt.includes(contextPlaceholder)) {
    sections.push(baseSystemPrompt.replace(contextPlaceholder, contextString));
  } else {
    sections.push(baseSystemPrompt);
    sections.push('');
    sections.push('─'.repeat(40));
    sections.push('');
    sections.push(contextString);
  }

  // 2. Phase instruction (always at the end for salience)
  sections.push('');
  sections.push('─'.repeat(40));
  sections.push('');
  sections.push(phaseInstruction);

  return sections.join('\n');
}

/**
 * Render the "Last Action" section for the system prompt.
 *
 * This captures what the assistant did in the previous step, including
 * any reasoning before a tool call. By putting this in the system prompt
 * (not as an assistant message), we avoid orphaned tool_call errors.
 */
function renderLastActionSection(state: ResearchState): string {
  const lines: string[] = [];

  // Only include if there's reasoning or observations to report
  if (!state.lastReasoning && state.pendingObservations.length === 0) {
    return '';
  }

  lines.push('## Last Action');
  lines.push('');

  // What the assistant was thinking/doing
  if (state.lastReasoning) {
    lines.push('### Reasoning');
    lines.push(state.lastReasoning);
    lines.push('');
  }

  // What tool was called (summarized, not the full message)
  if (state.pendingObservations.length > 0) {
    lines.push('### Tool Calls Made');
    for (const obs of state.pendingObservations) {
      lines.push(`- Called \`${obs.toolName}\`${obs.forQuestionId ? ` for question research` : ''}`);
    }
    lines.push('');
  }

  return lines.join('\n');
}

// =============================================================================
// Main Builder Function
// =============================================================================

/**
 * Build the message array for an LLM API call in research mode.
 *
 * Implements the "State-Only" pattern:
 * - Messages: [SystemMessage, UserMessage] only
 * - All context (facts, plan, observations, reasoning) in SystemMessage
 * - No assistant messages in history (avoids orphaned tool errors)
 *
 * @example
 * ```ts
 * const { messages } = buildTurnMessages({
 *   state: researchState,
 *   baseSystemPrompt: 'You are a helpful research assistant...',
 * });
 *
 * // messages = [
 * //   { role: 'system', content: '...base + context + phase instruction...' },
 * //   { role: 'user', content: 'Original research query' },
 * // ]
 * ```
 */
export function buildTurnMessages(options: BuildTurnMessagesOptions): TurnMessages {
  const {
    state,
    baseSystemPrompt,
    phaseInstruction,
    budget = DEFAULT_BUDGET,
    contextPlaceholder,
  } = options;

  // 1. Serialize state into token-budgeted context
  const contextInjection = serializeForPrompt(state, budget);

  // 2. Render context as string
  const contextString = renderContextForSystemPrompt(contextInjection);

  // 3. Render last action section (reasoning + tool calls summary)
  const lastActionSection = renderLastActionSection(state);

  // 4. Combine context with last action
  const fullContext = lastActionSection
    ? `${contextString}\n\n${lastActionSection}`
    : contextString;

  // 5. Get phase instruction (use override or default)
  let instruction = phaseInstruction ?? PHASE_INSTRUCTIONS[state.phase];
  
  // 6. Append manual termination note if user requested early wrap-up
  if (state.isManualTermination && state.phase === 'synthesizing') {
    instruction += `

📋 USER EARLY TERMINATION REQUEST
The user has requested early termination of this research session.
Synthesize the facts gathered so far into a coherent report.
Do NOT apologize for incomplete research or make excuses.
Focus on what WAS found and present it confidently.
Note any unanswered questions briefly as "Areas for further research" at the end.`;
  }

  // 7. Build complete system prompt
  const systemContent = buildSystemPrompt(
    baseSystemPrompt,
    fullContext,
    instruction,
    contextPlaceholder
  );

  // 7. Construct minimal message array
  const messages: TurnMessage[] = [
    { role: 'system', content: systemContent },
    { role: 'user', content: state.originalQuery },
  ];

  // 8. Calculate total chars for budget monitoring
  const totalChars = messages.reduce((sum, m) => sum + m.content.length, 0);

  return {
    messages,
    contextInjection,
    totalChars,
  };
}

// =============================================================================
// Utility Functions
// =============================================================================

/**
 * Estimate token count from character count.
 * Rough approximation: ~4 chars per token for English text.
 */
export function estimateTokens(chars: number): number {
  return Math.ceil(chars / 4);
}

/**
 * Check if messages are within a token budget.
 */
export function isWithinBudget(
  turnMessages: TurnMessages,
  maxTokens: number
): boolean {
  return estimateTokens(turnMessages.totalChars) <= maxTokens;
}

/**
 * Build messages with automatic budget adjustment.
 *
 * If initial build exceeds budget, progressively reduces serialization
 * budgets until it fits.
 */
export function buildTurnMessagesWithBudget(
  options: BuildTurnMessagesOptions,
  maxTokens: number
): TurnMessages {
  // Try with default budget first
  let result = buildTurnMessages(options);

  if (isWithinBudget(result, maxTokens)) {
    return result;
  }

  // Progressively reduce budget
  const reductionFactors = [0.75, 0.5, 0.25];

  for (const factor of reductionFactors) {
    const reducedBudget: SerializationBudget = {
      totalChars: Math.floor(DEFAULT_BUDGET.totalChars * factor),
      hypothesisChars: Math.floor(DEFAULT_BUDGET.hypothesisChars * factor),
      planChars: Math.floor(DEFAULT_BUDGET.planChars * factor),
      factsChars: Math.floor(DEFAULT_BUDGET.factsChars * factor),
      observationsChars: Math.floor(DEFAULT_BUDGET.observationsChars * factor),
      reasoningChars: Math.floor(DEFAULT_BUDGET.reasoningChars * factor),
    };

    result = buildTurnMessages({ ...options, budget: reducedBudget });

    if (isWithinBudget(result, maxTokens)) {
      console.warn(
        `[buildTurnMessages] Reduced budget to ${factor * 100}% to fit ${maxTokens} token limit`
      );
      return result;
    }
  }

  // If still over budget, return anyway with warning
  console.error(
    `[buildTurnMessages] Unable to fit within ${maxTokens} token budget. ` +
      `Current: ~${estimateTokens(result.totalChars)} tokens`
  );

  return result;
}

// =============================================================================
// Tool Definition Helpers
// =============================================================================

/**
 * Get the tool definitions to send with the API request.
 *
 * Only include tools during GATHERING phase - other phases should not
 * be calling tools and we don't want to confuse the model.
 */
export function shouldIncludeTools(phase: ResearchPhase): boolean {
  return phase === 'gathering';
}

/**
 * Filter tool definitions to only research-relevant tools.
 *
 * In research mode, we typically only want search/extraction tools,
 * not all available MCP tools.
 */
export function filterResearchTools<T extends { function: { name: string } }>(
  allTools: T[],
  allowedPrefixes: string[] = ['tavily', 'search', 'web', 'extract', 'fetch']
): T[] {
  return allTools.filter((tool) => {
    const name = tool.function.name.toLowerCase();
    return allowedPrefixes.some(
      (prefix) => name.includes(prefix.toLowerCase())
    );
  });
}

// =============================================================================
// Debug/Logging Helpers
// =============================================================================

/**
 * Create a debug summary of the turn messages.
 * Useful for logging without dumping entire prompts.
 */
export function summarizeTurnMessages(turn: TurnMessages): {
  systemPromptChars: number;
  userQueryChars: number;
  totalChars: number;
  estimatedTokens: number;
  phase: ResearchPhase;
  factsIncluded: number;
  questionsIncluded: number;
  hasObservations: boolean;
  hasReasoning: boolean;
} {
  const systemMsg = turn.messages.find((m) => m.role === 'system');
  const userMsg = turn.messages.find((m) => m.role === 'user');

  return {
    systemPromptChars: systemMsg?.content.length ?? 0,
    userQueryChars: userMsg?.content.length ?? 0,
    totalChars: turn.totalChars,
    estimatedTokens: estimateTokens(turn.totalChars),
    phase: turn.contextInjection.phase,
    factsIncluded: turn.contextInjection.progress.factsGathered,
    questionsIncluded: turn.contextInjection.progress.questionsTotal,
    hasObservations: turn.contextInjection.observations.length > 0,
    hasReasoning: turn.contextInjection.previousReasoning.length > 0,
  };
}
