/**
 * The chat-scoped overrides store, and the one thing about it that is easy to
 * get silently wrong: the two reasoning controls are **not** `AgentConfig`
 * fields.
 *
 * `POST /api/agent/chat` reads them from the top level of the body
 * (`AgentChatRequest::sampling_layer`), while `config` deserialises into
 * `AgentRequestConfig`, which declares neither. Serde drops unknown keys
 * without a word, so a level routed into `config` would be accepted by the
 * server, discarded, and leave no trace anywhere — the exact silent-discard
 * failure the reasoning arc exists to make impossible.
 */

import { describe, it, expect, beforeEach } from 'vitest';

import {
  agentOverridesToWire,
  reasoningOverridesToWire,
  writeStoredAgentOverrides,
  readStoredAgentOverrides,
} from '../../../src/services/agentOverrides';

describe('reasoning controls travel outside `config`', () => {
  beforeEach(() => localStorage.clear());

  it('keeps them out of the AgentConfig payload entirely', () => {
    writeStoredAgentOverrides({
      maxParallelTools: 4,
      reasoningEffort: 'high',
      reasoningBudgetTokens: 4096,
    });

    const config = agentOverridesToWire();

    expect(config).toEqual({ max_parallel_tools: 4 });
    expect(config).not.toHaveProperty('reasoning_effort');
    expect(config).not.toHaveProperty('reasoning_budget_tokens');
  });

  it('sends them under the snake_case names the request declares', () => {
    writeStoredAgentOverrides({ reasoningEffort: 'minimal', reasoningBudgetTokens: 256 });

    expect(reasoningOverridesToWire()).toEqual({
      reasoning_effort: 'minimal',
      reasoning_budget_tokens: 256,
    });
  });

  it('sends nothing when nothing is set, so every layer beneath still resolves', () => {
    expect(reasoningOverridesToWire()).toEqual({});
  });

  it('carries a zero budget rather than treating it as absent', () => {
    // 0 means "stop thinking", which is the only way to silence a reasoning
    // model whose template ignores the effort level. A falsy check here would
    // turn the strongest instruction in the pair into no instruction at all.
    writeStoredAgentOverrides({ reasoningBudgetTokens: 0 });

    expect(reasoningOverridesToWire()).toEqual({ reasoning_budget_tokens: 0 });
  });

  it('carries -1, which is a value and not an absence', () => {
    writeStoredAgentOverrides({ reasoningBudgetTokens: -1 });

    expect(reasoningOverridesToWire()).toEqual({ reasoning_budget_tokens: -1 });
  });

  it('lifts a stored value below the backend floor instead of 400-ing every chat', () => {
    // The server validates `>= -1` and answers -2 with an HTTP 400. A stored
    // value can outlive the UI that wrote it, so the clamp lives here too.
    writeStoredAgentOverrides({ reasoningBudgetTokens: -5000 });

    expect(reasoningOverridesToWire()).toEqual({ reasoning_budget_tokens: -1 });
  });

  it('round-trips both halves through storage', () => {
    writeStoredAgentOverrides({ reasoningEffort: 'max', reasoningBudgetTokens: -1 });

    expect(readStoredAgentOverrides()).toEqual({
      reasoningEffort: 'max',
      reasoningBudgetTokens: -1,
    });
  });
});
