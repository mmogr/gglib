/**
 * Every stream the app reads frames its events the same way.
 *
 * Four parsers used to read them (the event bus's and the proxy dashboard's,
 * a run's, the setup streams', the benchmark's) and each framed the same
 * bytes a little differently: one read nothing from a CRLF stream, two took
 * each line of a payload for a payload of its own. They all call the one
 * reader now, so each caller here is given the same payload framed five
 * ways and must hand over that payload, once, every time.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('../../../src/services/platform', async (original) => ({
  ...(await original<typeof import('../../../src/services/platform')>()),
  appLogger: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));

import { startCompareRun } from '../../../src/services/clients/benchmark';
import { subscribeProxyDashboard } from '../../../src/services/clients/proxyDashboard';
import { listenToServerLogs } from '../../../src/services/platform/serverLogs';
import { readRunEvents } from '../../../src/services/transport/api/runs';
import { streamLlamaInstall, streamLlamaUpdate } from '../../../src/services/transport/api/setup';
import { SSEConnectionManager } from '../../../src/services/transport/events/sse';

const encoder = new TextEncoder();

/** The payload every caller must hand over, and its JSON in two pieces that each end at a token. */
const PAYLOAD = { n: 1, text: 'héllo → 日本語 🚀' };
const HEAD = '{"n":1,';
const TAIL = '"text":"héllo → 日本語 🚀"}';
const JSON_TEXT = HEAD + TAIL;

/** `data: <payload>` as bytes, cut in two inside the two-byte `é`. */
function cutInsideACharacter(): Uint8Array[] {
  const bytes = encoder.encode(`data: ${JSON_TEXT}\n\n`);
  const at = encoder.encode(`data: ${JSON_TEXT.slice(0, JSON_TEXT.indexOf('é'))}`).length + 1;
  return [bytes.slice(0, at), bytes.slice(at)];
}

/** The same payload, as five streams: each entry is what one read returns. */
const FRAMINGS: [name: string, reads: (string | Uint8Array)[]][] = [
  ['a frame split across reads', [`data: ${HEAD}`, `${TAIL}\n`, '\n']],
  ['CRLF line endings', [`data: ${JSON_TEXT}\r\n\r\n`]],
  ['a data: field of several lines', [`data: ${HEAD}\ndata: ${TAIL}\n\n`]],
  ['a comment line before the event and one inside it', [': keepalive\n\n', `:ping\ndata: ${JSON_TEXT}\n\n`]],
  ['a character split across reads', cutInsideACharacter()],
];

type Seen = (payload: unknown) => void;

/** Each caller of the reader: start it, and answer what stops it. */
const CALLERS: [name: string, start: (seen: Seen, signal: AbortSignal) => unknown][] = [
  ['the event bus', (seen) => new SSEConnectionManager('/api/events').subscribe(seen)],
  ['the proxy dashboard', (seen) => subscribeProxyDashboard('127.0.0.1', 8080, null, seen)],
  ['the server log stream', (seen) => listenToServerLogs(9001, seen)],
  [
    'a run\'s events',
    (seen, signal) => {
      void (async () => {
        try {
          for await (const item of readRunEvents('r1', 0, signal)) {
            if (item.type === 'frame') seen(JSON.parse(item.data));
          }
        } catch {
          // the abort that ends the test
        }
      })();
    },
  ],
  ['the llama install stream', (seen) => streamLlamaInstall(seen, () => {})],
  ['the llama update stream', (seen) => streamLlamaUpdate(seen, () => {})],
  ['a benchmark stream', (seen, signal) => void startCompareRun({} as never, seen, signal).catch(() => {})],
];

describe.each(FRAMINGS)('a stream with %s', (_framing, reads) => {
  let controller: AbortController;
  let stop: unknown;

  beforeEach(() => {
    controller = new AbortController();
    // Serve the reads and stay open, as a live stream does, until the request's signal fires.
    vi.stubGlobal(
      'fetch',
      vi.fn(async (_url: string, init?: RequestInit) =>
        new Response(
          new ReadableStream<Uint8Array>({
            start(body) {
              for (const read of reads) body.enqueue(typeof read === 'string' ? encoder.encode(read) : read);
              init?.signal?.addEventListener('abort', () => body.error(init.signal!.reason));
            },
          }),
          { status: 200, headers: { 'content-type': 'text/event-stream' } },
        ),
      ),
    );
  });

  afterEach(async () => {
    controller.abort();
    if (typeof stop === 'function') stop();
    await new Promise((resolve) => setTimeout(resolve, 0));
    vi.unstubAllGlobals();
  });

  it.each(CALLERS)('reaches %s as the one payload it is', async (_caller, start) => {
    const seen: unknown[] = [];

    stop = await start((payload) => seen.push(payload), controller.signal);
    await vi.waitFor(() => expect(seen.length).toBeGreaterThan(0));
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(seen).toEqual([PAYLOAD]);
  });
});
