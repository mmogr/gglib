/**
 * Council of Agents — frontend domain types.
 *
 * Mirrors the Rust `council::events::CouncilEvent` discriminated union
 * and defines UI-layer state for the council session lifecycle.
 *
 * Wire types (`CouncilAgent`, `CouncilConfig`, `SuggestedCouncil`) live in
 * `services/clients/council.ts` alongside the HTTP/SSE client.
 *
 * @module types/council
 */

// Re-export wire types so consumers have a single import path.
export type {
  CouncilAgent,
  CouncilConfig,
  SuggestedCouncil,
} from '../services/clients/council';

// Import locally for use within this file.
import type { CouncilAgent } from '../services/clients/council';

// ─── SSE Event discriminated union ──────────────────────────────────────────

/**
 * Mirrors `council::events::CouncilEvent` (Rust).
 *
 * The `type` field is `serde(tag = "type", rename_all = "snake_case")` on the
 * Rust side, so every JSON event carries `"type": "agent_turn_start"` etc.
 */
export type CouncilEvent =
  | AgentTurnStartEvent
  | AgentTextDeltaEvent
  | AgentReasoningDeltaEvent
  | AgentToolCallStartEvent
  | AgentToolCallCompleteEvent
  | AgentTurnCompleteEvent
  | RoundSeparatorEvent
  | JudgeStartEvent
  | JudgeTextDeltaEvent
  | JudgeSummaryEvent
  | RoundCompactedEvent
  | StanceMapEvent
  | SynthesisStartEvent
  | SynthesisProgressEvent
  | AgentProgressEvent
  | SynthesisTextDeltaEvent
  | SynthesisCompleteEvent
  | CouncilErrorEvent
  | CouncilCompleteEvent;

export interface AgentTurnStartEvent {
  type: 'agent_turn_start';
  agent_id: string;
  agent_name: string;
  color: string;
  round: number;
  contentiousness: number;
}

export interface AgentTextDeltaEvent {
  type: 'agent_text_delta';
  agent_id: string;
  delta: string;
}

export interface AgentReasoningDeltaEvent {
  type: 'agent_reasoning_delta';
  agent_id: string;
  delta: string;
}

export interface AgentToolCallStartEvent {
  type: 'agent_tool_call_start';
  agent_id: string;
  tool_call: { name: string; arguments: string };
  display_name: string;
  args_summary?: string;
}

export interface AgentToolCallCompleteEvent {
  type: 'agent_tool_call_complete';
  agent_id: string;
  tool_name: string;
  result: { content: string; is_error: boolean };
  display_name: string;
  duration_display: string;
}

export interface AgentTurnCompleteEvent {
  type: 'agent_turn_complete';
  agent_id: string;
  content: string;
  round: number;
  core_claim?: string;
}

export interface RoundSeparatorEvent {
  type: 'round_separator';
  round: number;
}

export interface JudgeStartEvent {
  type: 'judge_start';
  round: number;
}

export interface JudgeTextDeltaEvent {
  type: 'judge_text_delta';
  delta: string;
}

export interface JudgeSummaryEvent {
  type: 'judge_summary';
  round: number;
  summary: string;
  consensus_reached: boolean;
}

export interface RoundCompactedEvent {
  type: 'round_compacted';
  round: number;
  summary: string;
}

export type StanceTrajectory = 'held' | 'shifted' | 'conceded';

export interface AgentStance {
  agent_name: string;
  trajectory: StanceTrajectory;
}

export interface StanceMapEvent {
  type: 'stance_map';
  stances: AgentStance[];
}

export interface SynthesisStartEvent {
  type: 'synthesis_start';
}

export interface SynthesisProgressEvent {
  type: 'synthesis_progress';
  processed: number;
  total: number;
  cached: number;
  time_ms: number;
}

export interface AgentProgressEvent {
  type: 'agent_progress';
  agent_id: string;
  processed: number;
  total: number;
  cached: number;
  time_ms: number;
}

export interface SynthesisTextDeltaEvent {
  type: 'synthesis_text_delta';
  delta: string;
}

export interface SynthesisCompleteEvent {
  type: 'synthesis_complete';
  content: string;
}

export interface CouncilErrorEvent {
  type: 'council_error';
  message: string;
}

export interface CouncilCompleteEvent {
  type: 'council_complete';
}

// ─── UI session state ───────────────────────────────────────────────────────

/** Completed contribution from a single agent turn. */
export interface AgentContribution {
  agentId: string;
  agentName: string;
  color: string;
  contentiousness: number;
  content: string;
  coreClaim?: string;
  round: number;
}

/** Tool call in progress or completed. */
export interface AgentToolCall {
  agentId: string;
  toolName: string;
  displayName: string;
  argsSummary?: string;
  result?: { content: string; isError: boolean };
  durationDisplay?: string;
}

/** Judge assessment for a single round. */
export interface JudgeAssessment {
  round: number;
  summary: string;
  consensusReached: boolean;
}

/** Summary of a compacted round. */
export interface CompactedRound {
  round: number;
  summary: string;
}

/** Session lifecycle phases. */
export type CouncilPhase =
  | 'idle'
  | 'suggesting'
  | 'setup'
  | 'deliberating'
  | 'judging'
  | 'synthesizing'
  | 'complete'
  | 'error';

/** Accumulated state for a single council session. */
export interface CouncilSession {
  phase: CouncilPhase;
  topic: string;
  /** Agents returned by the suggestion step. */
  suggestedAgents: CouncilAgent[];
  /** Recommended rounds from suggestion. */
  suggestedRounds: number;
  /** Optional synthesis guidance from suggestion. */
  suggestedSynthesisGuidance?: string;
  currentRound: number;
  totalRounds: number;
  /** Agent currently speaking (streaming). */
  activeAgentId: string | null;
  /** Name of the currently speaking agent. */
  activeAgentName: string;
  /** Hex color of the currently speaking agent. */
  activeAgentColor: string;
  /** Contentiousness of the currently speaking agent. */
  activeAgentContentiousness: number;
  /** Text accumulated for the active agent's current turn. */
  activeAgentText: string;
  /** Reasoning text accumulated for the active agent's current turn. */
  activeAgentReasoning: string;
  /** Active tool calls for the current agent turn. */
  activeToolCalls: AgentToolCall[];
  /** All completed contributions across rounds. */
  contributions: AgentContribution[];
  /** Judge assessments, one per evaluated round. */
  judgeAssessments: JudgeAssessment[];
  /** Judge text accumulated during current evaluation (streamed). */
  activeJudgeText: string;
  /** Round currently being evaluated by the judge. */
  activeJudgeRound: number;
  /** Stance trajectories from the post-debate evaluation. */
  stances: AgentStance[];
  /** Compacted round summaries. */
  compactedRounds: CompactedRound[];
  /** Synthesis text (streamed incrementally). */
  synthesisText: string;
  /** Pre-fill progress during synthesis (null when not in pre-fill). */
  synthesisProgress: { processed: number; total: number; cached: number; timeMs: number } | null;
  /** Error message if phase === 'error'. */
  error: string | null;
}

// ─── Contentiousness → colour mapping ───────────────────────────────────────

/**
 * Maps contentiousness float [0.0, 1.0] to a hex colour for ambient UI tinting.
 *
 * Tiers match the Rust `prompts.rs` behavioural descriptions and the CLI
 * ANSI-256 palette in `council.rs`:
 *
 * | Range     | Label           | Hex       |
 * |-----------|-----------------|-----------|
 * | 0.0–0.2   | Collaborative   | `#2d8d8d` |
 * | 0.2–0.4   | Constructive    | `#00af5f` |
 * | 0.4–0.6   | Balanced        | `#b2b2b2` |
 * | 0.6–0.8   | Adversarial     | `#ffaf00` |
 * | 0.8–1.0   | Devil's Advocate| `#ff0000` |
 */
export function contentiousnessColor(c: number): string {
  if (c < 0.2) return '#2d8d8d';
  if (c < 0.4) return '#00af5f';
  if (c < 0.6) return '#b2b2b2';
  if (c < 0.8) return '#ffaf00';
  return '#ff0000';
}

/** Human-readable label for the contentiousness tier. */
export function contentiousnessLabel(c: number): string {
  if (c < 0.2) return 'Collaborative';
  if (c < 0.4) return 'Constructive';
  if (c < 0.6) return 'Balanced';
  if (c < 0.8) return 'Adversarial';
  return "Devil's Advocate";
}

// ─── Serializable subset for DB persistence ────────────────────────────────

/**
 * Subset of CouncilSession stored in the DB metadata column.
 *
 * Strips transient streaming fields (activeAgent*, phase) to keep the
 * payload lean. Everything needed to render a historical council thread
 * is included.
 */
export interface SerializableCouncilSession {
  topic: string;
  totalRounds: number;
  contributions: AgentContribution[];
  synthesisText: string;
  /** Judge assessments, one per evaluated round. */
  judgeAssessments?: JudgeAssessment[];
  /** Stance trajectories from the post-debate evaluation. */
  stances?: AgentStance[];
  /** Compacted round summaries. */
  compactedRounds?: CompactedRound[];
  /** Non-null only for sessions that ended in error. */
  error?: string | null;
}

/** Extract the persistable subset from a live session. */
export function toSerializableSession(s: CouncilSession): SerializableCouncilSession {
  return {
    topic: s.topic,
    totalRounds: s.totalRounds,
    contributions: s.contributions,
    synthesisText: s.synthesisText,
    ...(s.judgeAssessments.length > 0 ? { judgeAssessments: s.judgeAssessments } : {}),
    ...(s.stances.length > 0 ? { stances: s.stances } : {}),
    ...(s.compactedRounds.length > 0 ? { compactedRounds: s.compactedRounds } : {}),
    ...(s.error ? { error: s.error } : {}),
  };
}

// ─── Factory ────────────────────────────────────────────────────────────────

/** Create a fresh idle session. */
export function createEmptySession(): CouncilSession {
  return {
    phase: 'idle',
    topic: '',
    suggestedAgents: [],
    suggestedRounds: 3,
    currentRound: 0,
    totalRounds: 0,
    activeAgentId: null,
    activeAgentName: '',
    activeAgentColor: '',
    activeAgentContentiousness: 0,
    activeAgentText: '',
    activeAgentReasoning: '',
    activeToolCalls: [],
    contributions: [],
    judgeAssessments: [],
    activeJudgeText: '',
    activeJudgeRound: 0,
    stances: [],
    compactedRounds: [],
    synthesisText: '',
    synthesisProgress: null,
    error: null,
  };
}
