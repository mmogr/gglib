/**
 * The event stream says when it opens, each time it opens.
 *
 * The stream has no backlog, and the daemon behind a reconnection may be a
 * new process. A listener holding state built from events, the download queue
 * for one, reads it again on this signal.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { SSEConnectionManager } from '../../../../src/services/transport/events/sse';

vi.mock('../../../../src/services/transport/api/client', () => ({
  getClient: async () => ({}),
  getApiBaseUrl: () => '',
  getAuthHeaders: () => ({}),
}));
vi.mock('../../../../src/services/transport/api/renew', () => ({
  renewAfterRefusal: async () => {},
}));

/** An accepted stream that sends `frames` and then ends. */
function stream(frames: string[]): Response {
  const encoder = new TextEncoder();
  return new Response(
    new ReadableStream<Uint8Array>({
      start(controller) {
        for (const frame of frames) controller.enqueue(encoder.encode(frame));
        controller.close();
      },
    }),
    { status: 200 },
  );
}

describe('the event stream\'s open signal', () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('fires for the first connection and again for each reconnection, before that connection\'s messages', async () => {
    // Two streams that each send one message and end, then one that stays open.
    fetchMock
      .mockImplementationOnce(async () => stream(['data: {"n":1}\n\n']))
      .mockImplementationOnce(async () => stream(['data: {"n":2}\n\n']))
      .mockImplementation(async () => new Response(new ReadableStream({ start() {} }), { status: 200 }));
    const manager = new SSEConnectionManager<{ n: number }>('/api/events');
    const seen: string[] = [];
    manager.opened.listen(() => seen.push('open'));

    // Listening for the open does not connect; a subscriber does.
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(fetchMock).not.toHaveBeenCalled();

    const stop = manager.subscribe((event) => seen.push(`message ${event.n}`));
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(3));
    await vi.waitFor(() => expect(seen).toHaveLength(5));
    stop();

    expect(seen).toEqual(['open', 'message 1', 'open', 'message 2', 'open']);
  });

  it('does not fire for a connection the server refuses', async () => {
    fetchMock
      .mockImplementationOnce(async () => new Response('no', { status: 503 }))
      .mockImplementation(async () => new Response(new ReadableStream({ start() {} }), { status: 200 }));
    const manager = new SSEConnectionManager('/api/events');
    const opened = vi.fn();
    manager.opened.listen(opened);

    const stop = manager.subscribe(() => {});
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
    expect(opened).not.toHaveBeenCalled();

    // It reconnects after its backoff, and that one is accepted.
    await vi.waitFor(() => expect(opened).toHaveBeenCalledTimes(1), { timeout: 3000 });
    expect(fetchMock).toHaveBeenCalledTimes(2);
    stop();
  });

  it('stops telling a listener that has let go', async () => {
    fetchMock.mockImplementation(async () => new Response(new ReadableStream({ start() {} }), { status: 200 }));
    const manager = new SSEConnectionManager('/api/events');
    const opened = vi.fn();
    manager.opened.listen(opened)();

    const stop = manager.subscribe(() => {});
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
    await new Promise((resolve) => setTimeout(resolve, 0));
    stop();

    expect(opened).not.toHaveBeenCalled();
  });
});
