/**
 * What an agent run's body carries.
 *
 * The body is hand-built rather than generated, so nothing but a test notices
 * a missing key. A turn on the paired machine's model names it by `far`, that
 * machine and the model's id there, and never by a name, which another model
 * there could share; a local turn names no model and lets llama-server serve
 * the one it loaded. The conversation's Thinking choice is a key of its own,
 * there only when the run changes it, beside the two reasoning controls and
 * never in place of them.
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
import type { ModelRef } from '../../../../src/types/generated/ModelRef';

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

  it('names a far model by its machine and its id there', () => {
    const far: ModelRef = { machine: { kind: 'paired', fingerprint: '3ca82708b995' }, id: 3 };
    const body = buildRunRequest(options({ selectedServerPort: undefined, far }));
    expect(body.far).toEqual(far);
    expect(body.model).toBeNull();
    expect(body.port).toBe(0);
  });

  it('locally names no model and no far one', () => {
    const body = buildRunRequest(options());
    expect(body.model).toBeNull();
    expect(body.far).toBeNull();
    expect(body.port).toBe(9000);
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

  it('says the Thinking choice only when given one, and changes nothing else by it', () => {
    const reasoning = { reasoning_effort: 'high', reasoning_budget_tokens: 2048 };
    const unsaid = buildRunRequest(options({ reasoning }));
    expect(Object.keys(unsaid)).not.toContain('thinking');
    expect(unsaid).toMatchObject({ reasoning_effort: 'high', reasoning_budget_tokens: 2048 });

    expect(buildRunRequest(options({ reasoning, thinking: 'off' }))).toEqual({ ...unsaid, thinking: 'off' });
    expect(buildRunRequest(options({ reasoning, thinking: 'default' }))).toEqual({ ...unsaid, thinking: 'default' });
  });

  it('says the Thinking choice of a turn on a far model as well', () => {
    const far: ModelRef = { machine: { kind: 'paired', fingerprint: '3ca82708b995' }, id: 3 };
    const body = buildRunRequest(options({ selectedServerPort: undefined, far, thinking: 'off' }));
    expect(body).toMatchObject({ far, thinking: 'off' });
  });
});

describe('mintRunId', () => {
  it('mints ids the hub accepts, and a new one each time', () => {
    const a = mintRunId();
    expect(a).toMatch(/^[A-Za-z0-9_-]{1,64}$/);
    expect(mintRunId()).not.toBe(a);
  });
});
