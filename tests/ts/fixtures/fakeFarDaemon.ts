/**
 * This machine's daemon with a far machine joined, behind `fetch`.
 *
 * `/api/remote/*` is answered as `handlers/remote/chats.rs` forwards it: the
 * far machine is a second `FakeDaemon` (`hub`), whose chats are listed with
 * the run live in each, opened with their rows, and continued with a turn
 * of `{content}` that the hub starts as its own run and saves. Every other
 * path goes to this machine's `FakeDaemon` (`here`). What the page sent to
 * the far routes is recorded in `farRequests`, as it sent it.
 */

import type { AgentRunRequest } from '../../../src/types/generated/AgentRunRequest';
import { FakeDaemon, type Recorded } from './fakeDaemon';

const LIVE = new Set(['queued', 'in_progress']);

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

export class FakeFarDaemon {
  here = new FakeDaemon();
  hub = new FakeDaemon();
  farRequests: Recorded[] = [];
  titles: Record<number, string> = { 1: 'Why the build broke', 2: 'Parsing GGUF' };

  /** How many far requests went to `method` on a path starting `prefix`. */
  farCount(method: string, prefix: string): number {
    return this.farRequests.filter((r) => r.method === method && r.url.startsWith(prefix)).length;
  }

  fetch = async (input: string | URL | Request, init: RequestInit = {}): Promise<Response> => {
    const url = String(input);
    if (!url.startsWith('/api/remote/')) return this.here.fetch(input, init);
    const method = init.method ?? 'GET';
    const body = typeof init.body === 'string' ? JSON.parse(init.body) : undefined;
    this.farRequests.push({ method, url, body });
    const [path, query] = url.split('?');
    const hub = (hubPath: string, hubInit: RequestInit = {}) =>
      this.hub.fetch(query ? `${hubPath}?${query}` : hubPath, { ...init, ...hubInit });
    let m: RegExpExecArray | null;

    if (method === 'GET' && path === '/api/remote/chats') {
      const live = [...this.hub.runs.values()].filter((r) => LIVE.has(r.info.status));
      const chats = [...this.hub.conversations].map((id) => {
        const run = live.find((r) => r.info.conversation_id === id);
        return {
          id,
          title: this.titles[id] ?? 'New Chat',
          updated_at: '2026-09-30 09:13:07',
          ...(run && { live_run: run.info.id }),
        };
      });
      return json({ chats });
    }
    if (method === 'GET' && (m = /^\/api\/remote\/chats\/(\d+)$/.exec(path))) {
      const id = Number(m[1]);
      if (!this.hub.conversations.has(id)) {
        return json({ error: 'no such chat', status: 404, type: 'not_found' }, 404);
      }
      const conversation = {
        id,
        title: this.titles[id] ?? 'New Chat',
        model_id: null,
        system_prompt: 'You are the hub.',
        created_at: '2026-09-30 09:12:30',
        updated_at: '2026-09-30 09:13:07',
      };
      return json({ conversation, messages: this.hub.saved(id) });
    }
    if (method === 'PUT' && (m = /^\/api\/remote\/chats\/(\d+)\/turns\/([^/]+)$/.exec(path))) {
      // As the hub takes a device's turn: the history is its own record.
      const request = {
        conversation_id: Number(m[1]),
        replace_from: null,
        messages: [{ role: 'user', content: (body as { content: string }).content }],
      } as unknown as AgentRunRequest;
      return this.hub.fetch(`/api/runs/${m[2]}?kind=agent`, { method: 'PUT', body: JSON.stringify(request) });
    }
    if (path === '/api/remote/runs') return hub('/api/runs');
    if ((m = /^\/api\/remote\/runs\/([^/]+)\/(events|cancel)$/.exec(path))) {
      return hub(`/api/runs/${m[1]}/${m[2]}`);
    }
    return json({ error: `no route ${method} ${path}`, status: 404, type: 'not_found' }, 404);
  };
}
