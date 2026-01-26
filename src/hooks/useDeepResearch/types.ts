/**
 * Deep Research Mode - Core Type Definitions
 *
 * Implements a "Plan-and-Execute" research architecture with:
 * - Structured ResearchState scratchpad (not chat history)
 * - Model routing for cheap extraction vs expensive reasoning
 * - Observation injection pattern (avoids orphaned tool_call errors)
 * - Persistence support via message_id linking
 *
 * @module useDeepResearch/types
 */

// =============================================================================
// Configuration & Model Routing
// =============================================================================

/**
 * Model endpoint configuration.
 * gglib manages model servers on ports - this identifies which to use.
 */
export interface ModelEndpoint {
  /** Server port where model is running */
  port: number;
  /** Optional: specific model ID if multiple loaded on same server */
  modelId?: number;
}

/**
 * Model routing configuration - allows cheap models for extraction.
 *
 * The "extraction cost trap": running fact extraction on the main reasoning
 * model is too slow/expensive. This allows routing extraction to a faster model.
 *
 * @example
 * ```ts
 * const routing: ModelRouting = {
 *   reasoningModel: { port: 8080 },      // Claude-3-opus or similar
 *   extractionModel: { port: 8081 },     // gpt-4o-mini or haiku
 * };
 * ```
 */
export interface ModelRouting {
  /** Model for planning and synthesis phases (expensive, capable) */
  reasoningModel: ModelEndpoint;
  /** Model for fact extraction from search results (cheap, fast) */
  extractionModel: ModelEndpoint;
  /** Model for summarization during pruning (cheap, fast). Falls back to extractionModel. */
  summarizationModel?: ModelEndpoint;
}

/**
 * Default routing when only one model is available.
 */
export function createDefaultRouting(port: number): ModelRouting {
  return {
    reasoningModel: { port },
    extractionModel: { port },
  };
}

// =============================================================================
// Research Plan & Questions
// =============================================================================

/**
 * Status of a research sub-question.
 */
export type QuestionStatus = 'pending' | 'in-progress' | 'answered' | 'blocked';

/**
 * A sub-question in the research plan.
 * Generated during PLANNING phase, answered during GATHERING phase.
 */
export interface ResearchQuestion {
  /** Unique identifier (UUID) */
  id: string;
  /** The sub-question text (max ~200 chars recommended) */
  question: string;
  /** Current status */
  status: QuestionStatus;
  /** Summary of answer when status='answered' (max ~500 chars) */
  answerSummary?: string;
  /** IDs of GatheredFacts that support this answer */
  supportingFactIds: string[];
  /** Priority for processing order (lower = higher priority) */
  priority: number;
  /** Parent question ID if this is a follow-up */
  parentQuestionId?: string;
  /** Step number when this question was marked in-progress (for timeout detection) */
  inProgressSince?: number;
}

/**
 * Create a new research question.
 */
export function createQuestion(
  question: string,
  priority: number = 0,
  parentId?: string
): ResearchQuestion {
  return {
    id: crypto.randomUUID(),
    question,
    status: 'pending',
    supportingFactIds: [],
    priority,
    parentQuestionId: parentId,
  };
}

// =============================================================================
// Gathered Facts (Knowledge Base)
// =============================================================================

/**
 * Confidence level for a gathered fact.
 */
export type FactConfidence = 'high' | 'medium' | 'low';

/**
 * A single fact extracted and attributed from search results.
 *
 * Raw source content is NEVER stored here - only distilled claims.
 * This is critical for staying within context limits across 30+ iterations.
 */
export interface GatheredFact {
  /** Unique identifier (UUID) */
  id: string;
  /** The factual claim (max ~200 chars) */
  claim: string;
  /** Source URL for citation */
  sourceUrl: string;
  /** Source title/name */
  sourceTitle: string;
  /** Confidence in this fact's accuracy */
  confidence: FactConfidence;
  /** Which research step gathered this fact */
  gatheredAtStep: number;
  /** IDs of questions this fact helps answer */
  relevantQuestionIds: string[];
  /** Optional: category/topic tag for grouping */
  category?: string;
}

/**
 * Create a new gathered fact.
 */
export function createFact(
  claim: string,
  sourceUrl: string,
  sourceTitle: string,
  confidence: FactConfidence,
  step: number,
  questionIds: string[] = []
): GatheredFact {
  return {
    id: crypto.randomUUID(),
    claim: claim.slice(0, 500), // Enforce max length
    sourceUrl,
    sourceTitle,
    confidence,
    gatheredAtStep: step,
    relevantQuestionIds: questionIds,
  };
}

// =============================================================================
// Contradictions & Gap Analysis
// =============================================================================

/**
 * A detected contradiction between two facts.
 */
export interface Contradiction {
  /** First conflicting fact ID */
  factIdA: string;
  /** Second conflicting fact ID */
  factIdB: string;
  /** Description of the contradiction */
  description: string;
  /** Whether this has been resolved */
  resolved: boolean;
  /** Resolution explanation if resolved */
  resolution?: string;
}

// =============================================================================
// Observation Injection (Solves Orphaned Tool Constraint)
// =============================================================================

/**
 * A pending observation from a tool call.
 *
 * These are processed into GatheredFacts, then the raw result is discarded.
 * The observation text is injected into the system prompt (NOT as tool messages),
 * which avoids the "orphaned tool_call" API validation error.
 */
export interface PendingObservation {
  /** Tool name (e.g., 'tavily_search', 'web_extract') */
  toolName: string;
  /** Tool call ID from the LLM response */
  toolCallId: string;
  /** Raw result from tool execution (kept temporarily) */
  rawResult: unknown;
  /** When this observation was received */
  timestamp: number;
  /** Which question this search was for (if applicable) */
  forQuestionId?: string;
}

// =============================================================================
// Research Phases
// =============================================================================

/**
 * Research execution phases.
 *
 * PLANNING:     Decompose query into sub-questions, form initial hypothesis
 * GATHERING:    Execute searches, extract facts, update hypothesis
 * SYNTHESIZING: Merge facts, resolve contradictions, generate final report
 * COMPLETE:     Research finished, final report available
 * ERROR:        Research failed (error stored in errorMessage)
 */
export type ResearchPhase =
  | 'planning'
  | 'gathering'
  | 'synthesizing'
  | 'complete'
  | 'error';

// =============================================================================
// Human-in-the-Loop Intervention
// =============================================================================

/**
 * Intervention signal for human-in-the-loop control.
 * Written to a MutableRefObject by UI, read by research loop.
 */
export type ResearchIntervention =
  | { type: 'wrap-up' }
  | { type: 'skip-question'; questionId: string };

/**
 * Type for the intervention ref passed to the research loop.
 */
export type InterventionRef = React.MutableRefObject<ResearchIntervention | null>;

// =============================================================================
// Verbose Execution Tracking (Activity Visibility)
// =============================================================================

/**
 * An active tool call being executed.
 * Used to show the user what searches/tools are running.
 */
export interface ActiveToolCall {
  /** Tool name (e.g., 'tavily_search', 'web_extract') */
  toolName: string;
  /** Tool call ID from the LLM response */
  toolCallId: string;
  /** Extracted search query if this is a search tool */
  searchQuery?: string;
  /** When this tool call started (Unix timestamp ms) */
  startedAt: number;
}

/**
 * Maximum number of entries in the activity log.
 */
export const MAX_ACTIVITY_LOG_ENTRIES = 5;

// =============================================================================
// Core Research State (The Scratchpad)
// =============================================================================

/**
 * The persistent research scratchpad.
 *
 * This is the ONLY state that persists across iterations. Raw tool outputs
 * are processed into this structure then discarded from message history.
 *
 * Designed to be:
 * 1. Serializable (stored in DB with message)
 * 2. Token-efficient (no raw search results)
 * 3. Resumable (can reload and continue)
 */
export interface ResearchState {
  // === Identity & Persistence ===
  /** Original user query that initiated this research */
  originalQuery: string;
  /** Message ID this state is attached to (for persistence) */
  messageId: string;
  /** Conversation ID */
  conversationId?: number;
  /** When research started (Unix timestamp ms) */
  startedAt: number;
  /** When research completed (Unix timestamp ms) */
  completedAt?: number;

  // === Plan (Updated by Planner) ===
  /** Working hypothesis, refined iteratively */
  currentHypothesis: string | null;
  /** Ordered list of sub-questions to answer */
  researchPlan: ResearchQuestion[];

  // === Knowledge Base (Append-only, Pruned) ===
  /** Gathered facts from searches (max ~50, LRU pruned) */
  gatheredFacts: GatheredFact[];

  // === Execution Tracking ===
  /** Current step number (1-indexed) */
  currentStep: number;
  /** Maximum steps allowed */
  maxSteps: number;
  /** Current execution phase */
  phase: ResearchPhase;

  // === Gap Analysis ===
  /** What we still don't know (for next iteration planning) */
  knowledgeGaps: string[];
  /** Detected contradictions between facts */
  contradictions: Contradiction[];

  // === Agent Memory (Prevents Amnesia) ===
  /**
   * Last reasoning/thought from the model before a tool call.
   * Injected into next turn so model knows WHY it requested current observations.
   */
  lastReasoning: string | null;

  // === Observation Buffer (Ephemeral) ===
  /**
   * Pending observations from tool calls - processed then cleared.
   * These are injected into system prompt as text, NOT as tool messages.
   */
  pendingObservations: PendingObservation[];

  // === Final Output ===
  /** Final research report (populated in COMPLETE phase) */
  finalReport: string | null;
  /** Citations used in final report */
  citations: Array<{
    factId: string;
    footnoteNumber: number;
  }>;

  // === Human-in-the-Loop Flags ===
  /** True if user manually triggered early termination via "Wrap Up" */
  isManualTermination?: boolean;

  // === Verbose Execution Tracking ===
  /** Activity log showing recent events (max 5, FIFO) */
  activityLog: string[];
  /** Currently executing tool calls with details */
  activeToolCalls: ActiveToolCall[];
  /** Whether LLM is currently generating (for "Thinking..." indicator) */
  isLLMGenerating: boolean;

  // === Error Handling ===
  /** Error message if phase='error' */
  errorMessage?: string;
}

/**
 * Create initial research state for a new query.
 */
export function createInitialState(
  query: string,
  messageId: string,
  options: {
    conversationId?: number;
    maxSteps?: number;
  } = {}
): ResearchState {
  return {
    // Identity
    originalQuery: query,
    messageId,
    conversationId: options.conversationId,
    startedAt: Date.now(),

    // Plan
    currentHypothesis: null,
    researchPlan: [],

    // Knowledge
    gatheredFacts: [],

    // Execution
    currentStep: 0,
    maxSteps: options.maxSteps ?? 30,
    phase: 'planning',

    // Gaps
    knowledgeGaps: [],
    contradictions: [],

    // Memory
    lastReasoning: null,
    pendingObservations: [],

    // Output
    finalReport: null,
    citations: [],

    // Verbose tracking
    activityLog: [],
    activeToolCalls: [],
    isLLMGenerating: false,
  };
}

// =============================================================================
// Serialization for Prompt Injection
// =============================================================================

/**
 * Token budget configuration for serialization.
 */
export interface SerializationBudget {
  /** Max chars for entire context injection (~4 chars/token) */
  totalChars: number;
  /** Max chars for hypothesis */
  hypothesisChars: number;
  /** Max chars for plan summary */
  planChars: number;
  /** Max chars for facts summary */
  factsChars: number;
  /** Max chars for observations */
  observationsChars: number;
  /** Max chars for last reasoning */
  reasoningChars: number;
}

/**
 * Default budget (~3k tokens total).
 */
export const DEFAULT_BUDGET: SerializationBudget = {
  totalChars: 12000,
  hypothesisChars: 1000,
  planChars: 2000,
  factsChars: 4000,
  observationsChars: 3000,
  reasoningChars: 1500,
};

/**
 * Serialized context ready for prompt injection.
 * This is what gets inserted into the system prompt each turn.
 */
export interface ResearchContextInjection {
  /** Formatted hypothesis section */
  hypothesis: string;
  /** Rendered research plan (questions with status) */
  planSummary: string;
  /** Top facts formatted with citations */
  factsSummary: string;
  /** Knowledge gaps as bullet list */
  gaps: string[];
  /** Recent observations formatted as text (NOT tool messages) */
  observations: string;
  /** Last reasoning for continuity */
  previousReasoning: string;
  /** Current phase */
  phase: ResearchPhase;
  /** Current focus question (in-progress) for explicit LLM guidance */
  currentFocus: {
    questionIndex: number;
    questionText: string;
  } | null;
  /** Progress indicator */
  progress: {
    step: number;
    maxSteps: number;
    questionsAnswered: number;
    questionsTotal: number;
    factsGathered: number;
  };
}

/**
 * Render a research question for prompt injection.
 * Includes Q{index} identifier that the LLM can reference when answering.
 */
function renderQuestion(q: ResearchQuestion, index: number): string {
  const statusIcon =
    q.status === 'answered'
      ? '✓'
      : q.status === 'in-progress'
        ? '→'
        : q.status === 'blocked'
          ? '✗'
          : '○';
  const answer = q.answerSummary ? ` — ${q.answerSummary}` : '';
  // Include Q{N} identifier for LLM to reference in AnswerResponse
  return `Q${index + 1}. [${statusIcon}] ${q.question}${answer}`;
}

/**
 * Render a gathered fact for prompt injection.
 */
function renderFact(f: GatheredFact, index: number): string {
  const conf = f.confidence === 'high' ? '●' : f.confidence === 'medium' ? '◐' : '○';
  return `[${index + 1}] ${conf} "${f.claim}" — ${f.sourceTitle}`;
}

/**
 * Render pending observations as system prompt text.
 * This is the key to avoiding orphaned tool_call errors.
 */
function renderObservations(
  observations: PendingObservation[],
  budget: number
): string {
  if (observations.length === 0) return '';

  const lines: string[] = ['## Recent Search Results'];

  for (const obs of observations) {
    const header = `### ${obs.toolName}${obs.forQuestionId ? ` (for question)` : ''}`;
    lines.push(header);

    // Stringify and truncate raw result
    let resultText: string;
    try {
      resultText =
        typeof obs.rawResult === 'string'
          ? obs.rawResult
          : JSON.stringify(obs.rawResult, null, 2);
    } catch {
      resultText = String(obs.rawResult);
    }

    // Truncate per-observation
    const perObsBudget = Math.floor(budget / Math.max(observations.length, 1));
    if (resultText.length > perObsBudget) {
      resultText = resultText.slice(0, perObsBudget - 20) + '\n... [truncated]';
    }

    lines.push(resultText);
    lines.push('');
  }

  return lines.join('\n').slice(0, budget);
}

/**
 * Serialize ResearchState into token-budgeted context for prompt injection.
 *
 * This is the core function that renders the scratchpad into text that
 * can be injected into the system prompt, staying within token limits.
 */
export function serializeForPrompt(
  state: ResearchState,
  budget: SerializationBudget = DEFAULT_BUDGET
): ResearchContextInjection {
  // === Hypothesis ===
  const hypothesis = state.currentHypothesis
    ? state.currentHypothesis.slice(0, budget.hypothesisChars)
    : '(No hypothesis formed yet)';

  // === Plan Summary ===
  const sortedQuestions = [...state.researchPlan].sort(
    (a, b) => a.priority - b.priority
  );
  const planLines = sortedQuestions.map(renderQuestion);
  let planSummary = planLines.join('\n');
  if (planSummary.length > budget.planChars) {
    // Prioritize showing in-progress and pending
    const inProgress = sortedQuestions.filter((q) => q.status === 'in-progress');
    const pending = sortedQuestions.filter((q) => q.status === 'pending');
    const answered = sortedQuestions.filter((q) => q.status === 'answered');

    const prioritized = [...inProgress, ...pending, ...answered];
    const truncatedLines: string[] = [];
    let charCount = 0;

    for (let i = 0; i < prioritized.length; i++) {
      const line = renderQuestion(prioritized[i], i);
      if (charCount + line.length > budget.planChars - 50) {
        truncatedLines.push(`... and ${prioritized.length - i} more questions`);
        break;
      }
      truncatedLines.push(line);
      charCount += line.length + 1;
    }
    planSummary = truncatedLines.join('\n');
  }

  // === Facts Summary (prioritize recent + referenced) ===
  const scoredFacts = state.gatheredFacts.map((f) => ({
    fact: f,
    score:
      (state.currentStep - f.gatheredAtStep) * -1 + // Recency (newer = higher)
      f.relevantQuestionIds.length * 2 + // Reference count
      (f.confidence === 'high' ? 3 : f.confidence === 'medium' ? 1 : 0),
  }));
  scoredFacts.sort((a, b) => b.score - a.score);

  const factLines: string[] = [];
  let factChars = 0;
  for (let i = 0; i < scoredFacts.length; i++) {
    const line = renderFact(scoredFacts[i].fact, i);
    if (factChars + line.length > budget.factsChars - 50) {
      factLines.push(`... and ${scoredFacts.length - i} more facts`);
      break;
    }
    factLines.push(line);
    factChars += line.length + 1;
  }
  const factsSummary = factLines.join('\n') || '(No facts gathered yet)';

  // === Observations ===
  const observations = renderObservations(
    state.pendingObservations,
    budget.observationsChars
  );

  // === Previous Reasoning ===
  const previousReasoning = state.lastReasoning
    ? state.lastReasoning.slice(0, budget.reasoningChars)
    : '';

  // === Progress ===
  const questionsAnswered = state.researchPlan.filter(
    (q) => q.status === 'answered'
  ).length;

  // === Current Focus (in-progress question) ===
  const inProgressQuestion = state.researchPlan.find(
    (q) => q.status === 'in-progress'
  );
  const currentFocus = inProgressQuestion
    ? {
        questionIndex: state.researchPlan.indexOf(inProgressQuestion) + 1,
        questionText: inProgressQuestion.question,
      }
    : null;

  return {
    hypothesis,
    planSummary,
    factsSummary,
    gaps: state.knowledgeGaps.slice(0, 5), // Max 5 gaps
    observations,
    previousReasoning,
    phase: state.phase,
    currentFocus,
    progress: {
      step: state.currentStep,
      maxSteps: state.maxSteps,
      questionsAnswered,
      questionsTotal: state.researchPlan.length,
      factsGathered: state.gatheredFacts.length,
    },
  };
}

/**
 * Render the full context injection as a string for the system prompt.
 */
export function renderContextForSystemPrompt(
  injection: ResearchContextInjection
): string {
  const sections: string[] = [];

  // Progress header
  const { progress } = injection;
  sections.push(
    `## Research Progress [Step ${progress.step}/${progress.maxSteps}] — Phase: ${injection.phase.toUpperCase()}`
  );
  sections.push(
    `Questions: ${progress.questionsAnswered}/${progress.questionsTotal} answered | Facts: ${progress.factsGathered} gathered`
  );
  sections.push('');

  // Current Focus (in-progress question) - critical for LLM context
  if (injection.currentFocus) {
    sections.push('## 🎯 Current Focus');
    sections.push(`You are currently working on **Q${injection.currentFocus.questionIndex}**: "${injection.currentFocus.questionText}"`);
    sections.push('');
    sections.push('When you have gathered enough information to answer this question, provide an AnswerResponse with `questionIndex: ' + injection.currentFocus.questionIndex + '`.');
    sections.push('');
  }

  // Previous reasoning (agent memory)
  if (injection.previousReasoning) {
    sections.push('## Previous Step Reasoning');
    sections.push(injection.previousReasoning);
    sections.push('');
  }

  // Current hypothesis
  sections.push('## Current Working Hypothesis');
  sections.push(injection.hypothesis);
  sections.push('');

  // Research plan
  sections.push('## Research Plan');
  sections.push(injection.planSummary);
  sections.push('');

  // Knowledge gaps
  if (injection.gaps.length > 0) {
    sections.push('## Knowledge Gaps');
    for (const gap of injection.gaps) {
      sections.push(`- ${gap}`);
    }
    sections.push('');
  }

  // Gathered facts
  sections.push('## Gathered Facts');
  sections.push(injection.factsSummary);
  sections.push('');

  // Recent observations (from tool calls)
  if (injection.observations) {
    sections.push(injection.observations);
  }

  return sections.join('\n');
}

// =============================================================================
// Deduplication Helpers
// =============================================================================

/** Similarity threshold for deduplication (0-1, higher = stricter) */
const DEDUP_SIMILARITY_THRESHOLD = 0.55;

/**
 * Normalize text for comparison.
 */
function normalizeText(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^\w\s]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/**
 * Tokenize text into words/ngrams for comparison.
 */
function tokenize(text: string): Set<string> {
  const normalized = normalizeText(text);
  const words = normalized.split(' ').filter((w) => w.length > 2);
  
  const tokens = new Set(words);
  for (let i = 0; i < words.length - 1; i++) {
    tokens.add(`${words[i]} ${words[i + 1]}`);
  }
  
  return tokens;
}

/**
 * Calculate Jaccard similarity between two token sets.
 */
function jaccardSimilarity(setA: Set<string>, setB: Set<string>): number {
  if (setA.size === 0 && setB.size === 0) return 1;
  if (setA.size === 0 || setB.size === 0) return 0;
  
  let intersection = 0;
  for (const token of setA) {
    if (setB.has(token)) intersection++;
  }
  
  const union = setA.size + setB.size - intersection;
  return intersection / union;
}

/**
 * Extract numeric values from text for comparison.
 */
function extractNumbers(text: string): string[] {
  const numbers: string[] = [];
  
  // Match percentages (including formatted like "1,153%")
  const percentRegex = /([0-9,]+(?:\.[0-9]+)?)\s*%/g;
  let match;
  while ((match = percentRegex.exec(text)) !== null) {
    numbers.push(match[1].replace(/,/g, ''));
  }
  
  // Match money values with multipliers
  const moneyRegex = /\$\s*([0-9,]+(?:\.[0-9]+)?)\s*(billion|million|thousand)?/gi;
  while ((match = moneyRegex.exec(text)) !== null) {
    let num = parseFloat(match[1].replace(/,/g, ''));
    const multiplier = (match[2] || '').toLowerCase();
    if (multiplier === 'billion') num *= 1e9;
    else if (multiplier === 'million') num *= 1e6;
    else if (multiplier === 'thousand') num *= 1e3;
    numbers.push(num.toString());
  }
  
  // Match multipliers (3.5x, 2x)
  const multiplierRegex = /([0-9]+(?:\.[0-9]+)?)\s*x\b/gi;
  while ((match = multiplierRegex.exec(text)) !== null) {
    numbers.push(match[1]);
  }
  
  return numbers;
}

/**
 * Check if two texts have conflicting numeric values.
 */
function hasNumericDivergence(textA: string, textB: string): boolean {
  const numsA = extractNumbers(textA);
  const numsB = extractNumbers(textB);
  
  if (numsA.length === 0 || numsB.length === 0) return false;
  
  const numericMatch = numsA.some(a => {
    const parsedA = parseFloat(a);
    return numsB.some(b => {
      const parsedB = parseFloat(b);
      const ratio = Math.abs(parsedA - parsedB) / Math.max(parsedA, parsedB, 1);
      return ratio < 0.1;
    });
  });
  
  return !numericMatch;
}

/**
 * Check if a fact claim is similar to an existing claim.
 */
function isSimilarClaim(claimA: string, claimB: string): boolean {
  const tokensA = tokenize(claimA);
  const tokensB = tokenize(claimB);
  const similarity = jaccardSimilarity(tokensA, tokensB);
  
  if (similarity >= DEDUP_SIMILARITY_THRESHOLD) {
    // Check for numeric divergence before marking as duplicate
    return !hasNumericDivergence(claimA, claimB);
  }
  return false;
}

// =============================================================================
// State Mutation Helpers
// =============================================================================

/**
 * Add facts to state with automatic deduplication and pruning.
 * 
 * Deduplication uses Jaccard similarity (threshold 0.55) with numeric-aware
 * comparison to prevent merging facts with different numbers (e.g., "40%" vs "1,153%").
 */
export function addFacts(
  state: ResearchState,
  newFacts: GatheredFact[],
  maxFacts: number = 50
): ResearchState {
  // Deduplicate new facts against existing facts
  const dedupedNewFacts: GatheredFact[] = [];
  let duplicateCount = 0;
  
  for (const newFact of newFacts) {
    const isDuplicate = state.gatheredFacts.some(
      (existing) => isSimilarClaim(newFact.claim, existing.claim)
    );
    
    if (isDuplicate) {
      console.log(
        `[addFacts] Discarding duplicate: "${newFact.claim.slice(0, 50)}..."`
      );
      duplicateCount++;
    } else {
      // Also check against facts we're adding in this batch
      const isBatchDuplicate = dedupedNewFacts.some(
        (added) => isSimilarClaim(newFact.claim, added.claim)
      );
      
      if (isBatchDuplicate) {
        console.log(
          `[addFacts] Discarding batch duplicate: "${newFact.claim.slice(0, 50)}..."`
        );
        duplicateCount++;
      } else {
        dedupedNewFacts.push(newFact);
      }
    }
  }
  
  if (duplicateCount > 0) {
    console.log(`[addFacts] Deduplication removed ${duplicateCount} duplicate facts`);
  }
  
  const combined = [...state.gatheredFacts, ...dedupedNewFacts];

  if (combined.length <= maxFacts) {
    return { ...state, gatheredFacts: combined };
  }

  // Prune: keep facts that support unanswered questions + highest scored
  const unansweredQuestionIds = new Set(
    state.researchPlan
      .filter((q) => q.status !== 'answered')
      .map((q) => q.id)
  );

  const scored = combined.map((f) => {
    const supportsUnanswered = f.relevantQuestionIds.some((id) =>
      unansweredQuestionIds.has(id)
    );
    return {
      fact: f,
      protected: supportsUnanswered,
      score:
        (state.currentStep - f.gatheredAtStep) * -0.5 +
        f.relevantQuestionIds.length * 2 +
        (f.confidence === 'high' ? 3 : f.confidence === 'medium' ? 1 : 0),
    };
  });

  // Sort: protected first, then by score
  scored.sort((a, b) => {
    if (a.protected !== b.protected) return a.protected ? -1 : 1;
    return b.score - a.score;
  });

  return {
    ...state,
    gatheredFacts: scored.slice(0, maxFacts).map((s) => s.fact),
  };
}

/**
 * Update a question's status and optionally set answer.
 */
export function updateQuestion(
  state: ResearchState,
  questionId: string,
  update: Partial<Pick<ResearchQuestion, 'status' | 'answerSummary' | 'supportingFactIds'>>
): ResearchState {
  return {
    ...state,
    researchPlan: state.researchPlan.map((q) =>
      q.id === questionId ? { ...q, ...update } : q
    ),
  };
}

/**
 * Clear pending observations (after extraction).
 */
export function clearObservations(state: ResearchState): ResearchState {
  return { ...state, pendingObservations: [] };
}

/**
 * Add a pending observation.
 */
export function addObservation(
  state: ResearchState,
  observation: Omit<PendingObservation, 'timestamp'>
): ResearchState {
  return {
    ...state,
    pendingObservations: [
      ...state.pendingObservations,
      { ...observation, timestamp: Date.now() },
    ],
  };
}

/**
 * Advance to next step.
 */
export function advanceStep(state: ResearchState): ResearchState {
  return { ...state, currentStep: state.currentStep + 1 };
}

/**
 * Set research phase.
 */
export function setPhase(state: ResearchState, phase: ResearchPhase): ResearchState {
  return { ...state, phase };
}

/**
 * Set error state.
 */
export function setError(state: ResearchState, message: string): ResearchState {
  return { ...state, phase: 'error', errorMessage: message };
}

/**
 * Complete research with final report.
 */
export function completeResearch(
  state: ResearchState,
  report: string,
  citations: ResearchState['citations']
): ResearchState {
  return {
    ...state,
    phase: 'complete',
    completedAt: Date.now(),
    finalReport: report,
    citations,
  };
}

// =============================================================================
// Activity Log Helpers
// =============================================================================

/**
 * Push an entry to the activity log (FIFO, max 5 entries).
 * Returns new state with updated log.
 */
export function pushActivityLog(
  state: ResearchState,
  message: string
): ResearchState {
  const newLog = [...state.activityLog, message];
  // Keep only the last N entries
  while (newLog.length > MAX_ACTIVITY_LOG_ENTRIES) {
    newLog.shift();
  }
  return { ...state, activityLog: newLog };
}

/**
 * Set active tool calls and optionally log the start.
 */
export function setActiveToolCalls(
  state: ResearchState,
  activeToolCalls: ActiveToolCall[],
  logSearchQueries: boolean = true
): ResearchState {
  let newState = { ...state, activeToolCalls };
  
  // Log search queries for visibility
  if (logSearchQueries) {
    for (const tc of activeToolCalls) {
      if (tc.searchQuery) {
        const truncated = tc.searchQuery.length > 50
          ? tc.searchQuery.slice(0, 47) + '...'
          : tc.searchQuery;
        newState = pushActivityLog(newState, `Searching: "${truncated}"`);
      } else {
        newState = pushActivityLog(newState, `Running ${tc.toolName}...`);
      }
    }
  }
  
  return newState;
}

/**
 * Clear active tool calls (after completion).
 */
export function clearActiveToolCalls(state: ResearchState): ResearchState {
  return { ...state, activeToolCalls: [] };
}

/**
 * Set LLM generating state.
 */
export function setLLMGenerating(
  state: ResearchState,
  isGenerating: boolean,
  logMessage?: string
): ResearchState {
  let newState = { ...state, isLLMGenerating: isGenerating };
  if (logMessage) {
    newState = pushActivityLog(newState, logMessage);
  }
  return newState;
}

// =============================================================================
// Validation
// =============================================================================

/**
 * Validate state integrity.
 */
export function validateState(state: ResearchState): {
  valid: boolean;
  errors: string[];
} {
  const errors: string[] = [];

  if (!state.originalQuery) {
    errors.push('Missing originalQuery');
  }

  if (!state.messageId) {
    errors.push('Missing messageId');
  }

  if (state.currentStep < 0) {
    errors.push('currentStep cannot be negative');
  }

  if (state.currentStep > state.maxSteps) {
    errors.push('currentStep exceeds maxSteps');
  }

  // Check fact references
  const factIds = new Set(state.gatheredFacts.map((f) => f.id));
  for (const q of state.researchPlan) {
    for (const factId of q.supportingFactIds) {
      if (!factIds.has(factId)) {
        errors.push(`Question ${q.id} references non-existent fact ${factId}`);
      }
    }
  }

  // Check contradiction references
  for (const c of state.contradictions) {
    if (!factIds.has(c.factIdA)) {
      errors.push(`Contradiction references non-existent fact ${c.factIdA}`);
    }
    if (!factIds.has(c.factIdB)) {
      errors.push(`Contradiction references non-existent fact ${c.factIdB}`);
    }
  }

  return { valid: errors.length === 0, errors };
}

// =============================================================================
// Persistence Types (for DB storage)
// =============================================================================

/**
 * Serialized state for database storage.
 * Stored as JSON in message metadata or dedicated research_state table.
 */
export type SerializedResearchState = string; // JSON.stringify(ResearchState)

/**
 * Serialize state for persistence.
 */
export function serializeState(state: ResearchState): SerializedResearchState {
  return JSON.stringify(state);
}

/**
 * Deserialize state from persistence.
 */
export function deserializeState(json: SerializedResearchState): ResearchState {
  return JSON.parse(json) as ResearchState;
}

// =============================================================================
// UI Progress Types (for Research Artifact component)
// =============================================================================

/**
 * Progress data for the collapsed Research Artifact view.
 */
export interface ResearchArtifactProgress {
  phase: ResearchPhase;
  phaseLabel: string;
  stepProgress: `${number}/${number}`;
  questionsProgress: `${number}/${number}`;
  factsCount: number;
  hasHypothesis: boolean;
  isComplete: boolean;
  hasError: boolean;
  errorMessage?: string;
}

/**
 * Extract progress data for UI rendering.
 */
export function getArtifactProgress(state: ResearchState): ResearchArtifactProgress {
  const phaseLabels: Record<ResearchPhase, string> = {
    planning: 'Planning research...',
    gathering: 'Gathering information...',
    synthesizing: 'Synthesizing findings...',
    complete: 'Research complete',
    error: 'Research failed',
  };

  const questionsAnswered = state.researchPlan.filter(
    (q) => q.status === 'answered'
  ).length;

  return {
    phase: state.phase,
    phaseLabel: phaseLabels[state.phase],
    stepProgress: `${state.currentStep}/${state.maxSteps}`,
    questionsProgress: `${questionsAnswered}/${state.researchPlan.length}`,
    factsCount: state.gatheredFacts.length,
    hasHypothesis: state.currentHypothesis !== null,
    isComplete: state.phase === 'complete',
    hasError: state.phase === 'error',
    errorMessage: state.errorMessage,
  };
}

// =============================================================================
// Re-exports for convenience
// =============================================================================

export const DEFAULT_SYSTEM_PROMPT = 'You are a helpful research assistant.';
