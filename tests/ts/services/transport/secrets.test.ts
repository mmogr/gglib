/**
 * What `revealSecret` sends, and what it hands back.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const { appLogger } = vi.hoisted(() => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

vi.mock('../../../../src/services/platform', () => ({ appLogger }));

import { revealSecret } from '../../../../src/services/transport/api/secrets';
import { readData } from '../../../../src/services/transport/errors';

const fetchMock = vi.fn();

function answer(value: string | null): Response {
  return new Response(JSON.stringify({ value }), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
}

describe('revealSecret', () => {
  beforeEach(() => {
    Object.values(appLogger).forEach((level) => level.mockClear());
    fetchMock.mockReset();
    vi.stubGlobal('fetch', fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('posts the reference to the reveal route and answers the value', async () => {
    fetchMock.mockResolvedValueOnce(answer('k3y'));

    const value = await revealSecret({ kind: 'mcp_env', server_id: 4, key: 'TOKEN' });

    expect(value).toBe('k3y');
    const [path, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(path).toBe('/api/secrets/reveal');
    expect(init.method).toBe('POST');
    expect(JSON.parse(init.body as string)).toStrictEqual({ kind: 'mcp_env', server_id: 4, key: 'TOKEN' });
  });

  it('answers null when nothing is stored', async () => {
    fetchMock.mockResolvedValueOnce(answer(null));

    expect(await revealSecret({ kind: 'setting', secret: 'proxy_api_key' })).toBeNull();
  });

  /** The debug log reaches the console and, on the desktop, the app's log. */
  it('writes the revealed value to no log', async () => {
    fetchMock.mockResolvedValueOnce(answer('k3y-5f3a9c'));

    expect(await revealSecret({ kind: 'setting', secret: 'proxy_api_key' })).toBe('k3y-5f3a9c');

    const logged = Object.values(appLogger).flatMap((level) => level.mock.calls);
    expect(logged.length).toBeGreaterThan(0);
    expect(JSON.stringify(logged)).not.toContain('k3y-5f3a9c');
  });

  /** The control: an answer that is not a secret is logged as it was. */
  it('logs an ordinary answer', async () => {
    await readData(answer('not-a-secret'));

    expect(JSON.stringify(appLogger.debug.mock.calls)).toContain('not-a-secret');
  });
});
