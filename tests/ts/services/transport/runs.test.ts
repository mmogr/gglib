/**
 * The runs door as the page speaks it: the PUT that starts an agent run, the
 * listing, the cancel, and a run's events read as numbered frames and one
 * final `run` event.
 */

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('../../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

import {
  cancelRun,
  listRuns,
  readRunEvents,
  startAgentRun,
  type RunStreamItem,
} from '../../../../src/services/transport/api/runs';
import type { AgentRunRequest } from '../../../../src/types/generated/AgentRunRequest';
import type { RunInfo } from '../../../../src/types/generated/RunInfo';

const fetchMock = vi.fn();

const info: RunInfo = {
  id: 'r1',
  kind: 'agent',
  status: 'in_progress',
  created_at_ms: 1,
  conversation_id: 4,
  last_seq: 0,
};

const request: AgentRunRequest = {
  conversation_id: 4,
  port: 9000,
  far: null,
  messages: [{ role: 'user', content: 'hi' }],
  config: null,
  tool_filter: null,
  model: null,
  reasoning_effort: null,
  reasoning_budget_tokens: null,
};

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

describe('runs transport', () => {
  beforeEach(() => {
    fetchMock.mockReset();
    vi.stubGlobal('fetch', fetchMock);
  });

  it('starts an agent run with a PUT under the minted id', async () => {
    fetchMock.mockResolvedValueOnce(json(info, 201));
    await expect(startAgentRun('r1', request)).resolves.toEqual(info);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('/api/runs/r1?kind=agent');
    expect(init.method).toBe('PUT');
    expect(JSON.parse(init.body as string)).toEqual(request);
  });

  it('a refused start rejects with the daemon\'s sentence', async () => {
    fetchMock.mockResolvedValueOnce(
      json({ error: 'all agent loop slots are in use; try again later', status: 429, type: 'agent_busy' }, 429),
    );
    await expect(startAgentRun('r1', request)).rejects.toThrow(
      'all agent loop slots are in use; try again later',
    );
  });

  it('lists the runs out of their envelope', async () => {
    fetchMock.mockResolvedValueOnce(json({ runs: [info] }));
    await expect(listRuns()).resolves.toEqual([info]);
    expect(fetchMock.mock.calls[0][0]).toBe('/api/runs');
  });

  it('cancels with a POST', async () => {
    fetchMock.mockResolvedValueOnce(json({ ...info, status: 'cancelled' }));
    await expect(cancelRun('r1')).resolves.toMatchObject({ status: 'cancelled' });
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('/api/runs/r1/cancel');
    expect(init.method).toBe('POST');
  });

  it('reads frames with their numbers, then the end, from the cursor asked for', async () => {
    const ended = { ...info, status: 'completed', last_seq: 2 };
    fetchMock.mockResolvedValueOnce(
      new Response(
        'id: 1\ndata: {"type":"text_delta","content":"a"}\n\n' +
          ':\n\n' +
          'id: 2\ndata: {"type":"final_answer","content":"a"}\n\n' +
          `event: run\ndata: ${JSON.stringify(ended)}\n\n`,
        { status: 200, headers: { 'content-type': 'text/event-stream' } },
      ),
    );
    const items: RunStreamItem[] = [];
    for await (const item of readRunEvents('r1', 0, new AbortController().signal)) {
      items.push(item);
    }
    expect(fetchMock.mock.calls[0][0]).toBe('/api/runs/r1/events?after=0');
    expect(items).toEqual([
      { type: 'frame', seq: 1, data: '{"type":"text_delta","content":"a"}' },
      { type: 'frame', seq: 2, data: '{"type":"final_answer","content":"a"}' },
      { type: 'end', info: ended },
    ]);
  });

  it('asks for an event stream, and passes the reader\'s signal to the request', async () => {
    fetchMock.mockResolvedValueOnce(new Response('', { status: 200 }));
    const signal = new AbortController().signal;
    for await (const item of readRunEvents('r 1', 3, signal)) void item;

    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('/api/runs/r%201/events?after=3');
    expect(init.headers).toEqual({ Accept: 'text/event-stream' });
    expect(init.signal).toBe(signal);
  });

  it('reads a preview as its own item, with no seq, between the frames (contracts/runs/preview_stream.txt)', async () => {
    const bytes = readFileSync(resolve(import.meta.dirname, '../../../../contracts/runs/preview_stream.txt'), 'utf8');
    fetchMock.mockResolvedValueOnce(new Response(bytes, { status: 200 }));
    const items: RunStreamItem[] = [];
    for await (const item of readRunEvents('r1', 0, new AbortController().signal)) items.push(item);

    expect(items.map((item) => item.type)).toEqual(['frame', 'preview', 'frame', 'end']);
    expect(items[0]).toMatchObject({ type: 'frame', seq: 1 });
    expect(items[1]).toEqual({
      type: 'preview',
      toolCallId: 'call_1',
      data: '{"tool_call_id":"call_1","frame":{"mime":"image/png","step":3,"total":20,"b64":"iVBORw0KGgo="}}',
    });
    expect(items[2]).toMatchObject({ type: 'frame', seq: 2 });
  });

  it('a preview whose data names no tool call is dropped, not read as a frame', async () => {
    fetchMock.mockResolvedValueOnce(
      new Response('event: preview\ndata: {"frame":{}}\n\n' + 'event: preview\ndata: nope\n\n', { status: 200 }),
    );
    const items: RunStreamItem[] = [];
    for await (const item of readRunEvents('r1', 0, new AbortController().signal)) items.push(item);

    expect(items).toEqual([]);
  });

  it('a keepalive sent as a ping payload is not a frame', async () => {
    fetchMock.mockResolvedValueOnce(
      new Response('data: ping\n\n' + 'id: 1\ndata: {"type":"text_delta","content":"a"}\n\n', { status: 200 }),
    );
    const items: RunStreamItem[] = [];
    for await (const item of readRunEvents('r1', 0, new AbortController().signal)) items.push(item);

    expect(items).toEqual([{ type: 'frame', seq: 1, data: '{"type":"text_delta","content":"a"}' }]);
  });

  it('a frame with no id is numbered 0, and nothing after the end is read', async () => {
    fetchMock.mockResolvedValueOnce(
      new Response(
        'data: {"type":"text_delta","content":"a"}\n\n' +
          `event: run\ndata: ${JSON.stringify(info)}\n\n` +
          'id: 9\ndata: {"type":"text_delta","content":"late"}\n\n',
        { status: 200 },
      ),
    );
    const items: RunStreamItem[] = [];
    for await (const item of readRunEvents('r1', 0, new AbortController().signal)) items.push(item);

    expect(items).toEqual([
      { type: 'frame', seq: 0, data: '{"type":"text_delta","content":"a"}' },
      { type: 'end', info },
    ]);
  });

  it('stops quietly, between reads, once its signal has fired', async () => {
    const encoder = new TextEncoder();
    const frames = ['id: 1\ndata: {"n":1}\n\n', 'id: 2\ndata: {"n":2}\n\n'];
    let next = 0;
    fetchMock.mockResolvedValueOnce(
      new Response(
        new ReadableStream<Uint8Array>({
          pull(body) {
            if (next < frames.length) body.enqueue(encoder.encode(frames[next++]));
            else body.close();
          },
        }),
        { status: 200 },
      ),
    );
    const controller = new AbortController();
    const items: RunStreamItem[] = [];
    for await (const item of readRunEvents('r1', 0, controller.signal)) {
      items.push(item);
      controller.abort();
    }

    expect(items).toEqual([{ type: 'frame', seq: 1, data: '{"n":1}' }]);
  });

  it('a run the daemon does not have rejects the read', async () => {
    fetchMock.mockResolvedValueOnce(json({ error: 'no run has id r9', status: 404 }, 404));
    const read = async () => {
      for await (const item of readRunEvents('r9', 0, new AbortController().signal)) {
        void item;
      }
    };
    await expect(read()).rejects.toThrow('no run has id r9');
  });
});
