/**
 * A server's log, as the log panel reads it: the lines so far in one
 * request, then the live stream.
 *
 * Both go through the transport's client, so both know where the daemon is
 * and hold its token however early they are called.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { appLogger } from '../../../src/services/platform';
import { getServerLogs, listenToServerLogs, type ServerLogEntry } from '../../../src/services/platform/serverLogs';
import { resetClientCache, setApiSession } from '../../../src/services/transport/api/client';
import { TransportError } from '../../../src/services/transport/errors';

const line = (n: number): ServerLogEntry => ({ timestamp: n, line: `line ${n}`, port: 9001 });

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });
}

describe('the server log', () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    resetClientCache();
    setApiSession('', 'page-key');
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    resetClientCache();
    setApiSession('', undefined);
  });

  it.each([
    ['the route\'s own shape', { logs: [line(1), line(2)] }],
    ['a bare list', [line(1), line(2)]],
    ['the enveloped shape', { success: true, data: { logs: [line(1), line(2)] } }],
  ])('reads the lines so far from %s', async (_shape, body) => {
    fetchMock.mockResolvedValue(json(body));

    await expect(getServerLogs(9001)).resolves.toEqual([line(1), line(2)]);

    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('/api/servers/9001/logs');
    expect(new Headers(init.headers).get('Authorization')).toBe('Bearer page-key');
  });

  it('answers no lines for a reply that holds none', async () => {
    fetchMock.mockResolvedValue(json({ something: 'else' }));

    await expect(getServerLogs(9001)).resolves.toEqual([]);
  });

  it('rejects with the daemon\'s coded error when the snapshot is refused', async () => {
    fetchMock.mockResolvedValue(json({ error: 'no server on port 9001', status: 404 }, 404));

    const error = await getServerLogs(9001).catch((e: unknown) => e);

    expect(error).toBeInstanceOf(TransportError);
    expect(error).toMatchObject({ code: 'NOT_FOUND', message: 'no server on port 9001' });
  });

  it('hands over each line of the live stream, and neither a keepalive nor a line that is not JSON', async () => {
    const encoder = new TextEncoder();
    fetchMock.mockResolvedValue(
      new Response(
        new ReadableStream<Uint8Array>({
          start(body) {
            body.enqueue(encoder.encode(`:ping\n\ndata: ping\n\ndata: ${JSON.stringify(line(1))}\n\n`));
            body.enqueue(encoder.encode(`data: not json\n\ndata: ${JSON.stringify(line(2))}\r\n\r\n`));
          },
        }),
        { status: 200 },
      ),
    );
    const seen: ServerLogEntry[] = [];
    const complaints = vi.spyOn(appLogger, 'error').mockImplementation(() => {});

    const stop = await listenToServerLogs(9001, (entry) => seen.push(entry));
    await vi.waitFor(() => expect(seen).toHaveLength(2));
    stop();

    expect(seen).toEqual([line(1), line(2)]);
    // The line that is not JSON is complained about; a keepalive is not.
    expect(complaints).toHaveBeenCalledTimes(1);
    expect(complaints).toHaveBeenCalledWith('service.server', 'Failed to parse log event', expect.objectContaining({ data: 'not json' }));
    complaints.mockRestore();
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('/api/servers/9001/logs/stream');
    expect(init.signal?.aborted).toBe(true);
  });
});
