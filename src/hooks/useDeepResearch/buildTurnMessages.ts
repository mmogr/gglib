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
  INTERNAL_RESEARCH_TOOLS,
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

STEP 1 - CLASSIFY COMPLEXITY:
Analyze the query and classify it as one of:
- "simple": Straightforward factual question (e.g., "What year was X founded?")
- "multi-faceted": Complex topic with multiple valid angles (e.g., "What are the pros and cons of X?")
- "controversial": Topic with competing viewpoints (e.g., "Is X better than Y?" or politically/ethically debated topics)

STEP 2 - GENERATE PERSPECTIVES (if not simple):
For "multi-faceted" or "controversial" queries, generate 2-3 distinct research perspectives:
- Each perspective represents a different lens to examine the topic
- For controversial topics: include opposing viewpoints (e.g., "Proponent view", "Critic view", "Neutral analyst")
- For multi-faceted topics: cover different angles (e.g., "Technical perspective", "Business perspective", "User perspective")

STEP 3 - CREATE RESEARCH PLAN:
1. Form an initial hypothesis based on your existing knowledge
2. Identify 3-7 distinct sub-questions that need answering
3. Identify what you DON'T know (knowledge gaps)

RESPOND WITH JSON:
{
  "type": "plan",
  "complexity": "simple" | "multi-faceted" | "controversial",
  "perspectives": ["Perspective 1", "Perspective 2"],
  "hypothesis": "Your initial working hypothesis...",
  "questions": [
    {"question": "Sub-question 1?", "priority": 1},
    {"question": "Sub-question 2?", "priority": 2}
  ],
  "gaps": ["What we don't know yet..."]
}

NOTES:
- For "simple" queries, "perspectives" should be an empty array []
- For "multi-faceted"/"controversial", provide 2-3 meaningful perspectives
- Each round of research will explore one perspective in depth

Do NOT use tools in planning phase. Output ONLY the JSON.`,

  gathering: `## Current Task: GATHERING

You are an ACTIVE researcher, not a passive executor. Search intelligently, reflect on progress, and pivot when needed.

CORE LOOP:
1. Search for information relevant to the current focus question
2. Every 3-4 tool calls, use \`assess_progress\` to reflect on coverage
3. If searches aren't yielding results, PIVOT your strategy
4. When you have sufficient evidence (minimum 4 facts), call \`request_synthesis\`

AVAILABLE ACTIONS:

1. SEARCH: Call tavily_search (or similar) with a specific, targeted query
   - Avoid broad queries - be specific
   - If a search returns nothing useful, try different keywords

2. ASSESS PROGRESS: Call \`assess_progress\` to reflect (do this often!)
   - What claims from the original query have evidence?
   - What gaps remain?
   - Should you pivot your search strategy?

3. ANSWER A QUESTION: When you have facts for the current focus, respond with JSON:
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

4. REQUEST SYNTHESIS: Call \`request_synthesis\` when research is complete
   - Requires minimum 4 facts gathered
   - Will be REJECTED if you haven't gathered enough evidence
   - Provide justification for why research is sufficient

IMPORTANT:
- "questionIndex" is the Q number from the Research Plan (Q1, Q2, etc.)
- Check the "🎯 Current Focus" section for the exact questionIndex
- Be an ACTIVE thinker: if something isn't working, change approach
- Don't repeat failed searches - pivot to different angles`,

  evaluating: `## Current Task: EVALUATING

You are assessing whether the gathered research adequately answers the original query.

INSTRUCTIONS:
1. Review the original query and all gathered facts
2. Assess how completely the research addresses the query (1-10 scale)
3. Identify specific aspects that are still missing or underexplored
4. Suggest targeted follow-up questions if more research would help

RESPOND WITH JSON:
{
  "type": "evaluation",
  "adequacyScore": 7,
  "assessment": "Brief explanation of the score...",
  "missingAspects": ["Aspect 1 not covered", "Aspect 2 needs more depth"],
  "suggestedFollowups": [
    {"question": "Follow-up question 1?", "priority": 1, "rationale": "Why this matters..."},
    {"question": "Follow-up question 2?", "priority": 2, "rationale": "Why this matters..."}
  ],
  "shouldContinue": true
}

SCORING GUIDELINES:
- 1-3: Critical gaps, key aspects unanswered
- 4-6: Partial coverage, significant gaps remain  
- 7-8: Good coverage, minor gaps acceptable
- 9-10: Comprehensive, ready for synthesis

Set "shouldContinue": false if score >= 7 OR if follow-ups would be redundant.
Do NOT use tools in evaluation phase. Output ONLY the JSON.`,

  compressing: `## Current Task: COMPRESSING

You are summarizing the current round's findings before starting a new research round.

INSTRUCTIONS:
1. Review all facts gathered in this round
2. Create a concise summary capturing the key findings (~500 chars max)
3. Focus on information most relevant to the original query

RESPOND WITH JSON:
{
  "type": "roundSummary",
  "summary": "Concise summary of this round's key findings...",
  "keyInsights": ["Most important insight 1", "Most important insight 2"]
}

Keep the summary factual and dense - it will be used as context for the next research round.
Do NOT use tools in compression phase. Output ONLY the JSON.`,

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
  
  // 5a. Inject perspective-specific instruction for gathering phase
  if (state.phase === 'gathering' && state.currentPerspective) {
    const perspectiveInstruction = `## 🎭 CURRENT PERSPECTIVE: "${state.currentPerspective}"

You are researching this topic from a specific angle: **${state.currentPerspective}**

Focus purely on finding evidence that supports or illuminates THIS perspective.
Do not worry about balancing viewpoints yet — other research rounds will cover other angles.
Seek sources and arguments that a researcher with this perspective would prioritize.

---

`;
    instruction = perspectiveInstruction + instruction;
  }
  
  // 5b. Inject perspective-aware evaluation rubric
  if (state.phase === 'evaluating' && state.currentPerspective) {
    const perspectiveEvaluation = `## 🎭 PERSPECTIVE-SPECIFIC EVALUATION

You are evaluating coverage for a SPECIFIC perspective: **${state.currentPerspective}**

Do NOT evaluate if the WHOLE query is answered yet.
Instead, evaluate: Have you adequately explored "${state.currentPerspective}"?

If this angle is well-covered with supporting evidence, score HIGH (7+).
Other research rounds will cover other perspectives.

---

`;
    instruction = perspectiveEvaluation + instruction;
  }
  
  // 5c. Inject multi-perspective synthesis instructions
  if (state.phase === 'synthesizing' && state.perspectives.length > 0) {
    const perspectivesList = state.perspectives.map((p, i) => `${i + 1}. ${p}`).join('\n');
    const roundPerspectives = state.roundSummaries
      .filter(rs => rs.perspective)
      .map(rs => `- Round ${rs.round}: "${rs.perspective}"`)
      .join('\n');
    
    const synthesisPerspectives = `## 🎭 MULTI-PERSPECTIVE SYNTHESIS

This research explored multiple distinct perspectives:
${perspectivesList}

Research rounds covered:
${roundPerspectives || '(No perspective-tagged rounds)'}

**CRITICAL INSTRUCTION**: Your final report MUST be structured to highlight these different viewpoints:
1. Create a section for each perspective explored, summarizing what evidence supports that angle
2. Note where perspectives agree and where they conflict
3. End with a "Synthesis" section that weighs the conflicting evidence and presents a balanced conclusion
4. Do NOT blend all facts together into one narrative — preserve the distinct viewpoints

---

`;
    instruction = synthesisPerspectives + instruction;
  }
  
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

/**
 * Get the complete set of research tools including internal agentic tools.
 * Appends assess_progress and request_synthesis to the filtered external tools.
 */
export function getResearchToolsWithInternals<T extends { type: string; function: { name: string } }>(
  allTools: T[],
  allowedPrefixes: string[] = ['tavily', 'search', 'web', 'extract', 'fetch']
): T[] {
  const externalTools = filterResearchTools(allTools, allowedPrefixes);
  // Cast internal tools to match the generic type (they're structurally compatible)
  return [
    ...externalTools,
    INTERNAL_RESEARCH_TOOLS[0] as unknown as T,
    INTERNAL_RESEARCH_TOOLS[1] as unknown as T,
  ];
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
