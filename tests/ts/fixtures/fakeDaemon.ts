/**
 * A daemon in memory, behind `fetch`, speaking the chat and runs routes as
 * `crates/gglib-axum/src/chat_api.rs` and `handlers/runs/` + `handlers/agent/`
 * do:
 *
 * - `POST /api/conversations` and `DELETE /api/messages/{id}` answer with a
 *   bare number.
 * - `GET /api/conversations/{id}/thread` answers the chat's rows, the branch
 *   points its family holds along them, and `answerable` when it ends in a
 *   question; `points` and `answerable` are left out when there are none.
 * - `POST /api/conversations/{id}/changes` makes a change as gglib's
 *   branching rules say (`fakeBranches.ts`): in place, or on a new chat of
 *   the same family holding a copy of the rows it keeps, each copy keyed by
 *   the row it copies as first written. A refusal changes nothing.
 * - `PUT /api/runs/{id}?kind=agent` answers `201` with the run `queued`, or
 *   `200` with the run that already has the id, saving nothing again. Once
 *   accepted it saves the request's last message when it is the user's; with
 *   `answer_saved`, none, and it is refused unless the chat ends in a
 *   question. A refusal changes nothing.
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
import type { ChatChange } from '../../../src/types/generated/ChatChange';
import type { RunInfo } from '../../../src/types/generated/RunInfo';
import { answerable, plan, points, REFUSED_STATUS, type LineChat, type PathRow } from './fakeBranches';
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
  /** The row each copy copies as first written; a row absent here is its own. */
  private origin = new Map<number, number>();
  /** The first chat of each branch's family; a chat absent here is its own. */
  private lineage = new Map<number, number>();
  /** When each chat last changed, as a count of saves. */
  private touched = new Map<number, number>();
  private saves = 0;
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
    this.touched.set(conversationId, ++this.saves);
    return saved;
  }

  /** The rows of `conversationId`, as the thread would list them. */
  saved(conversationId: number): ChatMessage[] {
    return this.rows.filter((r) => r.conversation_id === conversationId);
  }

  /** The rows of `conversationId` as the branching rules read them. */
  private path(conversationId: number): PathRow[] {
    return this.saved(conversationId).map((r) => ({ ...r, images: (r.images ?? []).map((image) => image.id) }));
  }

  /** `cid` and every other chat of its family, as the branch points read them. */
  private family(cid: number): LineChat[] {
    const root = (id: number) => this.lineage.get(id) ?? id;
    return [...this.conversations].filter((id) => root(id) === root(cid)).map((id) => ({
      conversation_id: id,
      updated_at: String(this.touched.get(id) ?? 0).padStart(9, '0'),
      rows: this.saved(id).map((r) => ({
        id: r.id,
        key: this.origin.get(r.id) ?? r.id,
        role: r.role,
        text: r.content,
        images: r.images?.length ?? 0,
      })),
    }));
  }

  /** Make `change` to `cid`, as `POST /api/conversations/{id}/changes` does. */
  private change(cid: number, change: ChatChange): Response {
    if (!this.conversations.has(cid)) return refusal(404, 'conversation_not_found', `no conversation has id ${cid}`);
    const busy = [...this.runs.values()].some(
      (r) => r.info.conversation_id === cid && (r.info.status === 'queued' || r.info.status === 'in_progress'),
    );
    const planned = plan(this.path(cid), change, busy);
    if ('refused' in planned) return refusal(REFUSED_STATUS[planned.refused], planned.refused, planned.refused);
    const images = change.kind === 'edit' ? this.images.infos(change.images ?? []) : [];
    const content = change.kind === 'edit' ? change.content : '';
    const question = { role: 'user' as const, content, ...(images.length > 0 && { images }) };
    const made = planned.plan;
    if ('replace' in made) {
      this.rows = this.rows.filter((r) => r.id !== made.replace.question);
      this.save(cid, question);
      return json({ conversation_id: cid, forked: false, answer: true });
    }
    const { through, then, answer } = made.fork;
    const branch = this.nextConversation++;
    this.conversations.add(branch);
    this.lineage.set(branch, this.lineage.get(cid) ?? cid);
    const kept = this.saved(cid);
    const end = through === null ? 0 : kept.findIndex((r) => r.id === through) + 1;
    kept.slice(0, end).forEach(({ id, conversation_id: _cid, created_at: _at, ...row }) => {
      this.origin.set(this.save(branch, row).id, this.origin.get(id) ?? id);
    });
    this.touched.set(branch, ++this.saves);
    if (then === 'question') this.save(branch, question);
    if (then === 'edited_reply') this.save(branch, { role: 'assistant', content, metadata: { edited: true } });
    return json({ conversation_id: branch, forked: true, answer });
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
    if (method === 'GET' && (m = /^\/api\/conversations\/(\d+)\/thread$/.exec(path))) {
      const cid = Number(m[1]);
      const along = points(cid, this.family(cid));
      return json({
        messages: this.saved(cid),
        ...(along.length > 0 && { points: along }),
        ...(answerable(this.path(cid)) && { answerable: true }),
      });
    }
    if (method === 'POST' && (m = /^\/api\/conversations\/(\d+)\/changes$/.exec(path))) {
      return this.change(Number(m[1]), body as ChatChange);
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
    if (request.answer_saved && (cid === null || !answerable(this.path(cid)))) {
      return refusal(409, 'nothing_to_answer', 'the conversation ends in no question to answer');
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
