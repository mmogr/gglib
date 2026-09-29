/**
 * A daemon in memory, behind `fetch`, speaking the chat and runs routes
 * with the bodies the real routes send.
 *
 * - `POST /api/conversations` and `DELETE /api/messages/{id}` answer with a
 *   bare number, as `chat_api.rs` does.
 * - `PUT /api/runs/{id}?kind=agent` saves the request's last message when it
 *   is the user's, as `handlers/agent/run.rs` does, and refuses as it does.
 * - A run's events are `id: <seq>` + `data: <frame>`, and its end is one
 *   `event: run` sent only once its reply is saved.
 *
 * A test drives a run with `emit` and `finish`. Every request is recorded.
 */

import type { ChatMessage } from '../../../src/services/transport';
import type { AgentRunRequest } from '../../../src/types/generated/AgentRunRequest';
import type { RunInfo } from '../../../src/types/generated/RunInfo';

export interface Recorded {
  method: string;
  url: string;
  body: unknown;
}

interface FakeRun {
  info: RunInfo;
  request: AgentRunRequest | null;
  frames: string[];
  streams: Array<ReadableStreamDefaultController<Uint8Array>>;
}

type NewRow = Omit<ChatMessage, 'id' | 'conversation_id' | 'created_at'>;

const encoder = new TextEncoder();

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function sse(text: string): Uint8Array {
  return encoder.encode(text);
}

export class FakeDaemon {
  rows: ChatMessage[] = [];
  conversations = new Set<number>([1, 2]);
  runs = new Map<string, FakeRun>();
  requests: Recorded[] = [];
  /** The answer the next run start gets instead of a run, once. */
  refuseNext: { status: number; type: string; error: string } | null = null;
  /** Holds every run start until it settles, when set. */
  startGate: Promise<void> | null = null;
  /** Readers whose `fetch` was aborted. */
  abortedReads = 0;
  private nextRow = 1;
  private nextConversation = 100;

  /** Save a row as the daemon would. */
  save(conversationId: number, row: NewRow): ChatMessage {
    const saved: ChatMessage = {
      id: this.nextRow++,
      conversation_id: conversationId,
      created_at: '2026-09-29T00:00:00Z',
      ...row,
    };
    this.rows.push(saved);
    return saved;
  }

  /** The rows of `conversationId`, as the thread would list them. */
  saved(conversationId: number): ChatMessage[] {
    return this.rows.filter((r) => r.conversation_id === conversationId);
  }

  /** A run already going, started elsewhere (another tab, before a reload). */
  running(id: string, conversationId: number, frames: object[] = []): FakeRun {
    const run: FakeRun = {
      info: {
        id,
        kind: 'agent',
        status: 'in_progress',
        created_at_ms: 1,
        conversation_id: conversationId,
        last_seq: 0,
      },
      request: null,
      frames: [],
      streams: [],
    };
    this.runs.set(id, run);
    frames.forEach((f) => this.emit(id, f));
    return run;
  }

  /** The one run started through the door, for a test that started one. */
  only(): FakeRun {
    const started = [...this.runs.values()].filter((r) => r.request !== null);
    if (started.length !== 1) throw new Error(`${started.length} runs were started`);
    return started[0];
  }

  /** Log one event to run `id` and send it to its readers. */
  emit(id: string, event: object): void {
    const run = this.runs.get(id)!;
    run.frames.push(JSON.stringify(event));
    run.info.last_seq = run.frames.length;
    const text = `id: ${run.frames.length}\ndata: ${JSON.stringify(event)}\n\n`;
    run.streams.forEach((s) => s.enqueue(sse(text)));
  }

  /** End run `id`: save `rows`, then tell its readers, then close. */
  finish(id: string, status: RunInfo['status'], rows: NewRow[] = []): void {
    const run = this.runs.get(id)!;
    const conversationId = run.info.conversation_id!;
    rows.forEach((row) => this.save(conversationId, row));
    run.info = { ...run.info, status, finished_at_ms: 2 };
    if (status === 'failed') run.info.error = { code: 'agent_error', message: 'The agent loop failed.' };
    const text = `event: run\ndata: ${JSON.stringify(run.info)}\n\n`;
    run.streams.forEach((s) => {
      s.enqueue(sse(text));
      s.close();
    });
    run.streams = [];
  }

  /** How many requests went to `method` on a path starting `prefix`. */
  count(method: string, prefix: string): number {
    return this.requests.filter((r) => r.method === method && r.url.startsWith(prefix)).length;
  }

  fetch = async (input: string | URL | Request, init: RequestInit = {}): Promise<Response> => {
    const url = String(input);
    const method = init.method ?? 'GET';
    const body = typeof init.body === 'string' ? JSON.parse(init.body) : undefined;
    this.requests.push({ method, url, body });
    const path = url.split('?')[0];
    let m: RegExpExecArray | null;

    if (method === 'POST' && path === '/api/conversations') {
      const id = this.nextConversation++;
      this.conversations.add(id);
      return json(id);
    }
    if (method === 'GET' && (m = /^\/api\/conversations\/(\d+)\/messages$/.exec(path))) {
      return json(this.saved(Number(m[1])));
    }
    if (method === 'DELETE' && (m = /^\/api\/messages\/(\d+)$/.exec(path))) {
      const target = this.rows.find((r) => r.id === Number(m![1]));
      if (!target) return json({ error: 'not found', status: 404 }, 404);
      const before = this.rows.length;
      this.rows = this.rows.filter(
        (r) => r.conversation_id !== target.conversation_id || r.id < target.id,
      );
      return json(before - this.rows.length);
    }
    if (method === 'PUT' && (m = /^\/api\/runs\/([^/]+)$/.exec(path))) {
      await this.startGate;
      return this.start(decodeURIComponent(m[1]), body as AgentRunRequest);
    }
    if (method === 'GET' && path === '/api/runs') {
      return json({ runs: [...this.runs.values()].map((r) => r.info) });
    }
    if (method === 'POST' && (m = /^\/api\/runs\/([^/]+)\/cancel$/.exec(path))) {
      const run = this.runs.get(decodeURIComponent(m[1]));
      if (!run) return json({ error: 'no such run', status: 404 }, 404);
      const text = run.frames
        .map((f) => JSON.parse(f) as { type: string; content?: string })
        .filter((e) => e.type === 'text_delta')
        .map((e) => e.content)
        .join('');
      this.finish(run.info.id, 'cancelled', [
        { role: 'assistant', content: text, metadata: { incomplete: true } },
      ]);
      return json(run.info);
    }
    if (method === 'GET' && (m = /^\/api\/runs\/([^/]+)\/events$/.exec(path))) {
      return this.events(decodeURIComponent(m[1]), url, init.signal ?? undefined);
    }
    return json({ error: `no route ${method} ${path}`, status: 404 }, 404);
  };

  private start(id: string, request: AgentRunRequest): Response {
    if (this.refuseNext) {
      const refusal = this.refuseNext;
      this.refuseNext = null;
      return json({ error: refusal.error, status: refusal.status, type: refusal.type }, refusal.status);
    }
    const cid = request.conversation_id;
    if (cid !== null && !this.conversations.has(cid)) {
      return json({ error: `no conversation has id ${cid}`, status: 404, type: 'conversation_not_found' }, 404);
    }
    const run = this.running(id, cid!);
    run.request = request;
    const last = request.messages[request.messages.length - 1];
    if (cid !== null && last?.role === 'user') this.save(cid, { role: 'user', content: last.content });
    return json(run.info, 201);
  }

  private events(id: string, url: string, signal?: AbortSignal): Response {
    const run = this.runs.get(id);
    if (!run) return json({ error: `no run has id ${id}`, status: 404 }, 404);
    const after = Number(new URL(url, 'http://daemon').searchParams.get('after') ?? 0);
    const ended = run.info.status !== 'in_progress' && run.info.status !== 'queued';
    const stream = new ReadableStream<Uint8Array>({
      start: (controller) => {
        run.frames.slice(after).forEach((frame, i) => {
          controller.enqueue(sse(`id: ${after + i + 1}\ndata: ${frame}\n\n`));
        });
        if (ended) {
          controller.enqueue(sse(`event: run\ndata: ${JSON.stringify(run.info)}\n\n`));
          controller.close();
          return;
        }
        run.streams.push(controller);
        signal?.addEventListener('abort', () => {
          this.abortedReads++;
          run.streams = run.streams.filter((s) => s !== controller);
          controller.error(new DOMException('The operation was aborted.', 'AbortError'));
        });
      },
    });
    return new Response(stream, { status: 200, headers: { 'content-type': 'text/event-stream' } });
  }
}
