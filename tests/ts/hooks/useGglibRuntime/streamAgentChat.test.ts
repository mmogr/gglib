/**
 * What the agent-chat request body carries, and what it refuses to send.
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

import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('../../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));
vi.mock('../../../../src/services/transport/api/client', () => ({
  getAuthenticatedFetchConfig: vi.fn(async () => ({
    baseUrl: 'http://127.0.0.1:9887',
    headers: {},
  })),
}));
vi.mock('../../../../src/services/tools', () => ({
  getToolRegistry: () => ({ getEnabledDefinitions: () => [], getBackendName: (n: string) => n }),
}));
// The stream itself is another file's subject; here the turn only has to end.
vi.mock('../../../../src/hooks/useGglibRuntime/agentSseReader', () => ({
  // eslint-disable-next-line require-yield
  readAgentSSE: async function* () {
    return;
  },
}));

import { streamAgentChat } from '../../../../src/hooks/useGglibRuntime/streamAgentChat';
import type { GglibMessage } from '../../../../src/types/messages';

const fetchMock = vi.fn();

/** Options with every knob at rest, so each test states only its own fields. */
function options(extra: Partial<Parameters<typeof streamAgentChat>[0]> = {}) {
  return {
    turnId: 'turn-1',
    getMessages: (): GglibMessage[] => [],
    setMessages: vi.fn(),
    selectedServerPort: 9000,
    mkAssistantMessage: () =>
      ({ id: 'assistant-1', role: 'assistant', content: [] }) as unknown as GglibMessage,
    ...extra,
  };
}

/** The JSON body of the single request the call made. */
function sentBody(): Record<string, unknown> {
  expect(fetchMock).toHaveBeenCalledTimes(1);
  const init = fetchMock.mock.calls[0][1] as RequestInit;
  return JSON.parse(init.body as string) as Record<string, unknown>;
}

describe('streamAgentChat request body', () => {
  beforeEach(() => {
    fetchMock.mockReset();
    fetchMock.mockResolvedValue(new Response(null, { status: 200 }));
    vi.stubGlobal('fetch', fetchMock);
  });

  it('sends the far machine the model it was told to ask for', async () => {
    await streamAgentChat(options({ remote: true, model: 'qwen3' }));
    expect(sentBody()).toMatchObject({ remote: true, model: 'qwen3' });
  });

  it('trims the name rather than sending the spaces around it', async () => {
    await streamAgentChat(options({ remote: true, model: '  qwen3  ' }));
    expect(sentBody().model).toBe('qwen3');
  });

  it('refuses a remote turn that names no model, before anything is sent', async () => {
    await expect(streamAgentChat(options({ remote: true }))).rejects.toThrow(
      /no model there is named/,
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('treats a name of only spaces as no name at all', async () => {
    await expect(streamAgentChat(options({ remote: true, model: '   ' }))).rejects.toThrow(
      /no model there is named/,
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('locally an absent model is the ordinary case and no key is sent', async () => {
    await streamAgentChat(options());
    const body = sentBody();
    expect(body).not.toHaveProperty('model');
    expect(body).not.toHaveProperty('remote');
    expect(body.port).toBe(9000);
  });

  it('locally a named model still travels', async () => {
    await streamAgentChat(options({ model: 'llama3' }));
    expect(sentBody()).toMatchObject({ model: 'llama3' });
  });
});
