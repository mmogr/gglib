/**
 * What an agent run's body carries, and what it refuses to send.
 *
 * The body is hand-built rather than generated, so nothing but a test notices
 * a missing key. It was missing `model` entirely: a turn aimed at the machine
 * on the other end of the tunnel arrived there as `"model": ""` — the far
 * proxy answered `404 Model '' not found`, a real answer through a working
 * tunnel that reads as the tunnel being broken.
 *
 * The local half is pinned in the same file, because the two paths mean
 * opposite things by an absent model and a fix to one is a plausible
 * regression in the other.
 */

import { describe, it, expect, vi } from 'vitest';

vi.mock('../../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));
vi.mock('../../../../src/services/tools', () => ({
  getToolRegistry: () => ({ getEnabledDefinitions: () => [], getBackendName: (n: string) => n }),
}));

import {
  buildRunRequest,
  mintRunId,
  type RunRequestOptions,
} from '../../../../src/hooks/useGglibRuntime/runRequest';
import type { GglibMessage } from '../../../../src/types/messages';

const hello = {
  id: 'u1',
  role: 'user',
  content: [{ type: 'text', text: 'hello' }],
} as unknown as GglibMessage;

/** Options with every knob at rest, so each test states only its own fields. */
function options(extra: Partial<RunRequestOptions> = {}): RunRequestOptions {
  return { messages: [hello], conversationId: 4, selectedServerPort: 9000, ...extra };
}

describe('buildRunRequest', () => {
  it('names the conversation the daemon saves to, and carries the history', () => {
    expect(buildRunRequest(options())).toMatchObject({
      conversation_id: 4,
      messages: [{ role: 'user', content: 'hello' }],
    });
  });

  it('sends the far machine the model it was told to ask for', () => {
    expect(buildRunRequest(options({ remote: true, model: 'qwen3' }))).toMatchObject({
      remote: true,
      model: 'qwen3',
    });
  });

  it('trims the name rather than sending the spaces around it', () => {
    expect(buildRunRequest(options({ remote: true, model: '  qwen3  ' })).model).toBe('qwen3');
  });

  it('refuses a remote turn that names no model', () => {
    expect(() => buildRunRequest(options({ remote: true }))).toThrow(/no model there is named/);
  });

  it('treats a name of only spaces as no name at all', () => {
    expect(() => buildRunRequest(options({ remote: true, model: '   ' }))).toThrow(
      /no model there is named/,
    );
  });

  it('locally an absent model is the ordinary case', () => {
    const body = buildRunRequest(options());
    expect(body.model).toBeNull();
    expect(body.remote).toBe(false);
    expect(body.port).toBe(9000);
  });

  it('locally a named model still travels', () => {
    expect(buildRunRequest(options({ model: 'llama3' })).model).toBe('llama3');
  });

  it('a model known not to call tools is offered none', () => {
    expect(buildRunRequest(options({ supportsToolCalls: false })).tool_filter).toEqual([]);
  });

  it('sends a config only when a field of it was set', () => {
    expect(buildRunRequest(options({ config: {} })).config).toBeNull();
    expect(buildRunRequest(options({ config: { max_iterations: 3 } })).config).toMatchObject({
      max_iterations: 3,
      max_parallel_tools: null,
    });
  });

  it('carries both reasoning controls at the top level', () => {
    const body = buildRunRequest(
      options({ reasoning: { reasoning_effort: 'high', reasoning_budget_tokens: 0 } }),
    );
    expect(body).toMatchObject({ reasoning_effort: 'high', reasoning_budget_tokens: 0 });
  });
});

describe('mintRunId', () => {
  it('mints ids the hub accepts, and a new one each time', () => {
    const a = mintRunId();
    expect(a).toMatch(/^[A-Za-z0-9_-]{1,64}$/);
    expect(mintRunId()).not.toBe(a);
  });
});
