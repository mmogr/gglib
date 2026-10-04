/**
 * A daemon in memory, behind `fetch`, speaking the chat and runs routes as
 * `crates/gglib-axum/src/chat_api.rs` and `handlers/runs/` + `handlers/agent/`
 * do:
 *
 * - `POST /api/conversations` and `DELETE /api/messages/{id}` answer with a
 *   bare number.
 * - `PUT /api/runs/{id}?kind=agent` answers `201` with the run `queued`, or
 *   `200` with the run that already has the id, saving nothing again. Once
 *   accepted it saves the request's last message when it is the user's; with
 *   `replace_from`, in place of that row and every later one. A refusal
 *   changes nothing.
 * - A run is `in_progress` from its first event. Its end (a `finish`, or a
 *   cancel) saves the reply after the call that asked for it returns, and
 *   only then does its status read as ended and its readers get the one
 *   `event: run`.
 * - `GET /api/runs` is newest first; absent optional fields are left out.
 * - `/api/attachments` is the image store (`fakeImageStore.ts`): a run
 *   whose messages name an image not stored, or over 16 MiB of images
 *   together, is refused by its code before it starts, and a saved user row
 *   lists its images' facts.
 *
 * A test drives a run with `emit` and `finish`, and can act at any request
 * with `before`. Every request is recorded.
 */

import type { ChatMessage } from '../../../src/services/transport';
import type { AgentRunRequest } from '../../../src/types/generated/AgentRunRequest';
import type { RunInfo } from '../../../src/types/generated/RunInfo';
import { FakeImageStore } from './fakeImageStore';

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
  ending: Promise<void> | null;
}

type NewRow = Omit<ChatMessage, 'id' | 'conversation_id' | 'created_at'>;

const encoder = new TextEncoder();

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function refusal(status: number, type: string, error: string): Response {
  return json({ error, status, type }, status);
}

function sse(text: string): Uint8Array {
  return encoder.encode(text);
}

export class FakeDaemon {
  rows: ChatMessage[] = [];
  conversations = new Set<number>([1, 2]);
  runs = new Map<string, FakeRun>();
  requests: Recorded[] = [];
  images = new FakeImageStore();
  /** The answer the next run start gets instead of a run, once. */
  refuseNext: { status: number; type: string; error: string } | null = null;
  /** The next run start never reaches the daemon: `fetch` rejects, once. */
  dropNextStart = false;
  /** Holds every run start until it settles, when set. */
  startGate: Promise<void> | null = null;
  /** Holds every run's end (its save, then its ending) until it settles, when set. */
  endGate: Promise<void> | null = null;
  /** How a cancelled run ends: `failed` when its reply could not be saved. */
  cancelEndsAs: 'cancelled' | 'failed' = 'cancelled';
  /** Runs whose events the daemon no longer has: a 404. */
  vanished = new Set<string>();
  /** Awaited before each request is answered. */
  before: ((method: string, path: string) => Promise<void> | void) | null = null;
  /** Readers whose `fetch` was aborted. */
  abortedReads = 0;
  private unsaved = new Set<string>();
  private nextRow = 1;
  private nextConversation = 100;
  private created = 0;

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
    const run = this.accept(id, conversationId);
    run.info.status = 'in_progress';
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
    if (run.info.status === 'queued') run.info.status = 'in_progress';
    run.frames.push(JSON.stringify(event));
    run.info.last_seq = run.frames.length;
    const text = `id: ${run.frames.length}\ndata: ${JSON.stringify(event)}\n\n`;
    run.streams.forEach((s) => s.enqueue(sse(text)));
  }

  /**
   * End run `id` as `status`. Its reply (`rows`) is saved after this
   * returns; then, at once, it reads as ended and its readers are told.
   */
  finish(id: string, status: RunInfo['status'], rows: NewRow[] = []): Promise<void> {
    const run = this.runs.get(id)!;
    run.ending ??= new Promise((resolve) => {
      setTimeout(async () => {
        await this.endGate;
        const conversationId = run.info.conversation_id!;
        rows.forEach((row) => this.save(conversationId, row));
        run.info = { ...run.info, status, finished_at_ms: 1790000008250 };
        if (status === 'failed') {
          run.info.error = { code: 'agent_error', message: 'The agent loop failed.' };
        }
        if (this.unsaved.has(id)) {
          run.info.error = { code: 'transcript_not_saved', message: 'The reply could not be saved to its conversation.' };
        }
        const text = `event: run\ndata: ${JSON.stringify(run.info)}\n\n`;
        run.streams.forEach((s) => {
          s.enqueue(sse(text));
          s.close();
        });
        run.streams = [];
        resolve();
      }, 0);
    });
    return run.ending;
  }

  /** How many requests went to `method` on a path starting `prefix`. */
  count(method: string, prefix: string): number {
    return this.requests.filter((r) => r.method === method && r.url.startsWith(prefix)).length;
  }

  fetch = async (input: string | URL | Request, init: RequestInit = {}): Promise<Response> => {
    const url = String(input);
    const method = init.method ?? 'GET';
    // JSON as the page sends it; an image's raw bytes as the `Blob` it sent.
    const body = typeof init.body === 'string' ? JSON.parse(init.body) : (init.body ?? undefined);
    this.requests.push({ method, url, body });
    const path = url.split('?')[0];
    await this.before?.(method, path);
    let m: RegExpExecArray | null;

    if (method === 'POST' && path === '/api/attachments') return this.images.upload(body);
    if (method === 'GET' && (m = /^\/api\/attachments\/([^/]+)$/.exec(path))) {
      return this.images.read(decodeURIComponent(m[1]));
    }
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
      if (!target) return refusal(404, 'not_found', 'Message not found');
      const before = this.rows.length;
      this.rows = this.rows.filter(
        (r) => r.conversation_id !== target.conversation_id || r.id < target.id,
      );
      return json(before - this.rows.length);
    }
    if (method === 'PUT' && (m = /^\/api\/runs\/([^/]+)$/.exec(path))) {
      if (this.dropNextStart) {
        this.dropNextStart = false;
        throw new TypeError('Failed to fetch');
      }
      await this.startGate;
      return this.start(decodeURIComponent(m[1]), body as AgentRunRequest);
    }
    if (method === 'GET' && path === '/api/runs') {
      const newest = [...this.runs.values()].reverse();
      return json({ runs: newest.map((r) => r.info) });
    }
    if (method === 'POST' && (m = /^\/api\/runs\/([^/]+)\/cancel$/.exec(path))) {
      const run = this.runs.get(decodeURIComponent(m[1]));
      if (!run) return refusal(404, 'run_not_found', 'no such run');
      // As the route answers: going, until the reply is saved.
      const ended = run.info.status === 'completed' || run.info.status === 'failed' || run.info.status === 'cancelled';
      const shown: RunInfo = ended ? { ...run.info } : { ...run.info, status: 'in_progress' };
      const text = run.frames
        .map((f) => JSON.parse(f) as { type: string; content?: string })
        .filter((e) => e.type === 'text_delta')
        .map((e) => e.content)
        .join('');
      if (this.cancelEndsAs === 'failed') {
        this.unsaved.add(run.info.id);
        void this.finish(run.info.id, 'failed');
      } else {
        void this.finish(run.info.id, 'cancelled', [
          { role: 'assistant', content: text, metadata: { incomplete: true } },
        ]);
      }
      return json(shown);
    }
    if (method === 'GET' && (m = /^\/api\/runs\/([^/]+)\/events$/.exec(path))) {
      return this.events(decodeURIComponent(m[1]), url, init.signal ?? undefined);
    }
    return refusal(404, 'not_found', `no route ${method} ${path}`);
  };

  private accept(id: string, conversationId: number | null): FakeRun {
    const info: RunInfo = {
      id,
      kind: 'agent',
      status: 'queued',
      model: 'qwen',
      created_at_ms: 1790000000000 + this.created++,
      last_seq: 0,
    };
    if (conversationId !== null) info.conversation_id = conversationId;
    const run: FakeRun = { info, request: null, frames: [], streams: [], ending: null };
    this.runs.set(id, run);
    return run;
  }

  private start(id: string, request: AgentRunRequest): Response {
    const existing = this.runs.get(id);
    if (existing) return json(existing.info, 200);
    if (this.refuseNext) {
      const { status, type, error } = this.refuseNext;
      this.refuseNext = null;
      return refusal(status, type, error);
    }
    const cid = request.conversation_id;
    const unreadable = this.images.check(request.messages as Array<{ images?: string[] }>);
    if (unreadable) return unreadable;
    if (cid !== null && !this.conversations.has(cid)) {
      return refusal(404, 'conversation_not_found', `no conversation has id ${cid}`);
    }
    const last = request.messages[request.messages.length - 1];
    const from = request.replace_from;
    if (from !== null) {
      const target = this.rows.find((r) => r.id === from && r.conversation_id === cid);
      if (!target) {
        return refusal(404, 'message_not_found', `conversation ${cid} has no message ${from} to replace`);
      }
      this.rows = this.rows.filter((r) => r.conversation_id !== cid || r.id < from);
    }
    const run = this.accept(id, cid);
    run.request = request;
    if (cid !== null && last?.role === 'user') {
      const images = last.images ?? [];
      this.save(cid, { role: 'user', content: last.content, ...(images.length > 0 && { images: this.images.infos(images) }) });
    }
    return json(run.info, 201);
  }

  private events(id: string, url: string, signal?: AbortSignal): Response {
    const run = this.runs.get(id);
    if (!run || this.vanished.has(id)) return refusal(404, 'run_not_found', `no run has id ${id}`);
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
