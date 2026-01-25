/**
 * Robust agent loop implementation with ReAct-lite protocol enforcement.
 *
 * Provides loop detection, memory compression, retry policies, and protocol
 * enforcement without prompt spam. Designed for thorough, Copilot-style tool use.
 *
 * @module agentLoop
 */

import type { AccumulatedToolCall } from './accumulateToolCalls';
import type { ToolResult } from '../../services/tools';

// =============================================================================
// Configuration
// =============================================================================

/** Agent policy knobs */
export const DEFAULT_MAX_TOOL_ITERS = 25;
export const MAX_PROTOCOL_STRIKES = 2;
export const MAX_SAME_SIGNATURE_HITS = 2;
export const MAX_STAGNATION_STEPS = 5;

/** Memory management */
export const MAX_CONTEXT_CHARS = 180_000;
export const KEEP_LAST_TOOL_MESSAGES = 10;
export const TOOL_RESULT_SNIPPET_CHARS = 4_000;

// =============================================================================
// Types
// =============================================================================

export interface ChatMessage {
  role: 'system' | 'user' | 'assistant' | 'tool';
  content: string | null;
  tool_call_id?: string;
  tool_calls?: Array<{
    id: string;
    type: string;
    function: { name: string; arguments: string };
  }>;
}

export interface FinalEnvelope {
  type: 'final';
  result: string;
  checks?: string[];
}

export interface ToolDigest {
  sig: string;
  name: string;
  ok: boolean;
  summary: string;
}

export interface AgentLoopState {
  iter: number;
  protocolStrikes: number;
  stagnation: number;
  sigHits: Map<string, number>;
  lastAssistantHash?: string;
  toolDigests: ToolDigest[];
}

// =============================================================================
// Hashing & Signatures
// =============================================================================

/**
 * Stable JSON stringify for consistent hashing.
 */
function stableStringify(v: unknown): string {
  const seen = new WeakSet();
  return JSON.stringify(v, function (_key, value) {
    if (value && typeof value === 'object') {
      if (seen.has(value as object)) return '[Circular]';
      seen.add(value as object);
      if (!Array.isArray(value)) {
        return Object.keys(value as Record<string, unknown>)
          .sort()
          .reduce((acc, k) => {
            (acc as Record<string, unknown>)[k] = (value as Record<string, unknown>)[k];
            return acc;
          }, {} as Record<string, unknown>);
      }
    }
    return value;
  });
}

/**
 * Fast non-crypto hash for loop detection.
 */
function hashString(s: string): string {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return (h >>> 0).toString(16);
}

/**
 * Generate signature for a tool call (name + args hash).
 */
export function toolSignature(tc: AccumulatedToolCall): string {
  const argStr = tc.function.arguments ?? '';
  return `${tc.function.name}:${hashString(argStr)}`;
}

// =============================================================================
// Final Envelope Parsing
// =============================================================================

/**
 * Try to parse final envelope from assistant content.
 * Returns null if content is not a valid final envelope.
 */
export function tryParseFinalEnvelope(content: string): FinalEnvelope | null {
  const trimmed = content.trim();
  if (!trimmed.startsWith('{')) return null;
  try {
    const parsed = JSON.parse(trimmed);
    if (parsed?.type === 'final' && typeof parsed.result === 'string') {
      return {
        type: 'final',
        result: parsed.result,
        checks: Array.isArray(parsed.checks)
          ? parsed.checks.filter((x: unknown) => typeof x === 'string')
          : [],
      };
    }
    return null;
  } catch {
    return null;
  }
}

// =============================================================================
// Retry Policy
// =============================================================================

/**
 * Check if error is transient and retryable.
 */
function isTransientError(e: unknown): boolean {
  const msg = String((e as { message?: string })?.message ?? e ?? '');
  return (
    msg.includes('ECONNRESET') ||
    msg.includes('ETIMEDOUT') ||
    msg.includes('fetch failed') ||
    msg.includes('NetworkError') ||
    msg.includes('503') ||
    msg.includes('502')
  );
}

/**
 * Retry a function with exponential backoff.
 */
export async function withRetry<T>(
  fn: () => Promise<T>,
  opts: {
    maxRetries: number;
    baseDelayMs: number;
    shouldRetry?: (e: unknown) => boolean;
  }
): Promise<T> {
  const { maxRetries, baseDelayMs, shouldRetry } = opts;
  let attempt = 0;
  while (true) {
    try {
      return await fn();
    } catch (e) {
      attempt++;
      const retryable =
        (shouldRetry ? shouldRetry(e) : isTransientError(e)) &&
        attempt <= maxRetries;
      if (!retryable) throw e;
      const delay = baseDelayMs * Math.pow(2, attempt - 1);
      await new Promise((r) => setTimeout(r, delay));
    }
  }
}

// =============================================================================
// Working Memory & Context Management
// =============================================================================

/**
 * Summarize a tool result for working memory.
 */
export function summarizeToolResult(_name: string, res: ToolResult): string {
  if (!res.success) {
    return `ERROR: ${res.error}`.slice(0, TOOL_RESULT_SNIPPET_CHARS);
  }
  const raw = stableStringify(res.data);
  return raw.slice(0, TOOL_RESULT_SNIPPET_CHARS);
}

/**
 * Build working memory system message from tool digests.
 */
export function buildWorkingMemory(digests: ToolDigest[]): string {
  const lines: string[] = ['WORKING_MEMORY:'];
  for (const d of digests.slice(-KEEP_LAST_TOOL_MESSAGES)) {
    lines.push(`- ${d.name} (${d.ok ? 'ok' : 'fail'}): ${d.summary}`);
  }
  return lines.join('\n');
}

/**
 * Upsert working memory system message in conversation.
 */
export function upsertWorkingMemory(
  messages: ChatMessage[],
  memory: string
): ChatMessage[] {
  const idx = messages.findIndex(
    (m) => m.role === 'system' && m.content?.startsWith('WORKING_MEMORY:')
  );
  const memMsg: ChatMessage = { role: 'system', content: memory };

  if (idx === -1) {
    // Insert after first system message if present, else at top
    const firstSys = messages.findIndex((m) => m.role === 'system');
    if (firstSys >= 0) {
      const out = messages.slice();
      out.splice(firstSys + 1, 0, memMsg);
      return out;
    }
    return [memMsg, ...messages];
  }

  const out = messages.slice();
  out[idx] = memMsg;
  return out;
}

/**
 * Calculate total character count of messages.
 */
function totalChars(messages: ChatMessage[]): number {
  return messages.reduce((acc, m) => acc + (m.content?.length ?? 0), 0);
}

/**
 * Prune messages to stay under context budget.
 */
export function pruneForBudget(messages: ChatMessage[]): ChatMessage[] {
  if (totalChars(messages) <= MAX_CONTEXT_CHARS) return messages;

  const isWorkingMemory = (m: ChatMessage) =>
    m.role === 'system' && m.content?.startsWith('WORKING_MEMORY:');

  const toolMsgs = messages.filter((m) => m.role === 'tool');
  const keepToolIds = new Set(
    toolMsgs.slice(-KEEP_LAST_TOOL_MESSAGES).map((m) => m.tool_call_id)
  );

  let out = messages.filter((m) => {
    if (isWorkingMemory(m)) return true;
    if (m.role !== 'tool') return true;
    return keepToolIds.has(m.tool_call_id);
  });

  // If still too big, keep system messages + last few turns
  if (totalChars(out) > MAX_CONTEXT_CHARS) {
    const system = out.filter((m) => m.role === 'system');
    const nonSystem = out.filter((m) => m.role !== 'system');
    const tail = nonSystem.slice(-12);
    out = [...system, ...tail];
  }

  return out;
}

// =============================================================================
// Loop Detection & Progress Tracking
// =============================================================================

/**
 * Record assistant progress and check for stagnation.
 */
export function recordAssistantProgress(
  state: AgentLoopState,
  assistantContent: string
): AgentLoopState {
  const h = hashString(assistantContent.trim());
  const sameAsLast = state.lastAssistantHash === h;
  return {
    ...state,
    lastAssistantHash: h,
    stagnation: sameAsLast ? state.stagnation + 1 : 0,
  };
}

/**
 * Check for tool call loops (same signature repeated).
 */
export function checkToolLoop(
  state: AgentLoopState,
  toolCalls: AccumulatedToolCall[]
): { loopDetected: boolean; updatedState: AgentLoopState } {
  const sigs = toolCalls.map(toolSignature).sort();
  const batchSig = sigs.join('|');
  const prevHits = state.sigHits.get(batchSig) ?? 0;

  const updatedState = {
    ...state,
    sigHits: new Map(state.sigHits).set(batchSig, prevHits + 1),
  };

  return {
    loopDetected: prevHits + 1 > MAX_SAME_SIGNATURE_HITS,
    updatedState,
  };
}

// =============================================================================
// Protocol
// =============================================================================

export const FORMAT_REMINDER = `You must respond in ONE of these ways:
1) If you need tools: return tool_calls (no long narrative).
2) If finished: output ONLY this JSON:
{"type":"final","result":"...","checks":["..."]}`;

/** System prompt for tool-enabled models (agent/reasoning with Jinja) */
export const TOOL_ENABLED_SYSTEM_PROMPT = `You are an assistant with tools.

Rules:
- If you need information or actions, use tool_calls. Do not guess.
- Keep explanations brief while working; prefer tool use.
- When you are done, respond ONLY with JSON:
  {"type":"final","result":"...","checks":["verified ...","not verified ..."]}

Do not output chain-of-thought.`;

/** System prompt for non-tool models (plain chat) */
export const DEFAULT_SYSTEM_PROMPT = 'You are a helpful assistant.';

/**
 * Get appropriate system prompt based on tool availability.
 */
export function getSystemPrompt(hasTools: boolean): string {
  return hasTools ? TOOL_ENABLED_SYSTEM_PROMPT : DEFAULT_SYSTEM_PROMPT;
}
