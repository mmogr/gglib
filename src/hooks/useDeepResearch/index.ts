/**
 * Deep Research Mode
 *
 * Implements "Plan-and-Execute" research with structured state management,
 * multi-model routing, and observation injection patterns.
 *
 * @module useDeepResearch
 */

// Core types and state management
export type {
  // Configuration
  ModelEndpoint,
  ModelRouting,
  SerializationBudget,
  // Research plan
  QuestionStatus,
  ResearchQuestion,
  // Knowledge base
  FactConfidence,
  GatheredFact,
  Contradiction,
  // Observations
  PendingObservation,
  // Core state
  ResearchPhase,
  ResearchState,
  // Human-in-the-loop
  ResearchIntervention,
  InterventionRef,
  // Serialization
  ResearchContextInjection,
  SerializedResearchState,
  // UI
  ResearchArtifactProgress,
} from './types';

export {
  // Factory functions
  createDefaultRouting,
  createQuestion,
  createFact,
  createInitialState,
  // Serialization
  DEFAULT_BUDGET,
  serializeForPrompt,
  renderContextForSystemPrompt,
  serializeState,
  deserializeState,
  // State mutations
  addFacts,
  updateQuestion,
  clearObservations,
  addObservation,
  advanceStep,
  setPhase,
  setError,
  completeResearch,
  // Validation
  validateState,
  // UI helpers
  getArtifactProgress,
} from './types';

// Turn message construction
export type {
  TurnMessage,
  TurnMessages,
  BuildTurnMessagesOptions,
} from './buildTurnMessages';

export {
  PHASE_INSTRUCTIONS,
  buildTurnMessages,
  buildTurnMessagesWithBudget,
  estimateTokens,
  isWithinBudget,
  shouldIncludeTools,
  filterResearchTools,
  summarizeTurnMessages,
} from './buildTurnMessages';

// Research loop orchestrator
export type {
  ToolDefinition,
  ToolCall,
  ToolResult,
  ToolExecutor,
  LLMResponse,
  LLMCaller,
  RunResearchLoopOptions,
  ResearchLoopResult,
} from './runResearchLoop';

export {
  DEFAULT_MAX_STEPS,
  SOFT_LANDING_THRESHOLD,
  MAX_PARALLEL_TOOLS,
  TOOL_TIMEOUT_MS,
  runResearchLoop,
  createProxyLLMCaller,
  createRegistryToolExecutor,
} from './runResearchLoop';

// Fact extraction
export type {
  ExtractionLLMCaller,
  ExtractFactsOptions,
  ExtractFactsResult,
} from './factExtractor';

export {
  DEDUP_SIMILARITY_THRESHOLD,
  MAX_FACTS_RETAINED,
  MAX_FACTS_PER_OBSERVATION,
  extractFacts,
  pruneFacts,
  calculateSimilarity,
  wouldBeDuplicate,
} from './factExtractor';

// Main hook for React integration
export type {
  UseDeepResearchOptions,
  UseDeepResearchReturn,
} from './useDeepResearch';

export { useDeepResearch } from './useDeepResearch';

