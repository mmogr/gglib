/**
 * This machine's daemon with a far machine joined, behind `fetch`.
 *
 * `/api/remote/*` is answered as `handlers/remote/chats.rs` forwards it: the
 * far machine is a second `FakeDaemon` (`hub`), whose chats are listed with
 * the run live in each, opened with their rows, and continued with a turn
 * of `{content, images?}` that the hub starts as its own run and saves; its
 * images go to the hub's store at `/api/remote/attachments`, and a far
 * gglib from before images (`noImages`) answers that route 404 and a turn
 * that names one `400 invalid_request`. Every other
 * path goes to this machine's `FakeDaemon` (`here`). What the page sent to
 * the far routes is recorded in `farRequests`, as it sent it.
 *
 * `/api/remote/models*` is answered as `handlers/remote/models.rs` reads it:
 * `PairedModels` with the machine, the paired row of the actions table
 * (`list`, `detail`, `chat`, `load`) and every entry the far `/v1/models`
 * lists, variants included; one model's `ModelLookup`, with no `filePath`
 * and no `port`, as the far detail route strips them; a model it does not
 * have as its `404` with `model_not_found`; and a load, after which that
 * model is serving.
 */

import type { AgentRunRequest } from '../../../src/types/generated/AgentRunRequest';
import type { LoadResponse } from '../../../src/types/generated/LoadResponse';
import type { ModelDetailDto } from '../../../src/types/generated/ModelDetailDto';
import type { ModelInfo } from '../../../src/types/generated/ModelInfo';
import type { ModelLookup } from '../../../src/types/generated/ModelLookup';
import type { PairedModels } from '../../../src/types/generated/PairedModels';
import { FakeDaemon, type Recorded } from './fakeDaemon';

/** The fingerprint of the far machine these models are on. */
export const FAR_FINGERPRINT = '3ca82708b995';

/** A far `/v1/models` entry: a base entry unless `profile` is given. */
export function farEntry(name: string, gglibId: number, extra: Partial<ModelInfo> = {}): ModelInfo {
  return {
    id: extra.profile ? `${name}:${extra.profile}` : name,
    gglib_id: gglibId,
    object: 'model',
    created: 1_727_000_000,
    owned_by: 'gglib',
    description: 'qwen3 - 8B parameters, Q4_K_M',
    context_window: 30_000,
    ...extra,
  };
}

/** `GET /api/remote/models` as the daemon answers it. */
export function pairedModels(models: ModelInfo[]): PairedModels {
  return {
    machine: { kind: 'paired', fingerprint: FAR_FINGERPRINT },
    actions: ['list', 'detail', 'chat', 'load'],
    models,
  };
}

/** One far model's detail, as the far detail route strips it: no path, no port. */
export function farDetail(id: number, name: string, extra: Partial<ModelDetailDto> = {}): ModelDetailDto {
  return {
    id,
    name,
    imageInput: false,
    paramCountB: 8,
    architecture: 'qwen3',
    quantization: 'Q4_K_M',
    contextLength: 40_960,
    tags: [],
    capabilities: 0,
    reasoningEffortSupport: 'unknown',
    addedAt: '2026-09-30 09:12:30',
    isServing: false,
    metadata: {},
    ...extra,
  };
}

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
  /** How many more listings of the far chats fail, as a dropped tunnel would. */
  listFails = 0;
  /** The far machine's models, by its id there. */
  models: Record<number, ModelDetailDto> = { 3: farDetail(3, 'qwen3') };
  /** What its `/v1/models` lists, in its order. */
  entries: ModelInfo[] = [farEntry('qwen3', 3), farEntry('qwen3', 3, { profile: 'coding' })];
  /** How many more readings of the far models fail. */
  modelsFail = 0;
  /** The far machine's gglib predates images. */
  noImages = false;

  /** How many far requests went to `method` on a path starting `prefix`. */
  farCount(method: string, prefix: string): number {
    return this.farRequests.filter((r) => r.method === method && r.url.startsWith(prefix)).length;
  }

  fetch = async (input: string | URL | Request, init: RequestInit = {}): Promise<Response> => {
    const url = String(input);
    if (!url.startsWith('/api/remote/')) return this.here.fetch(input, init);
    const method = init.method ?? 'GET';
    const body = typeof init.body === 'string' ? JSON.parse(init.body) : (init.body ?? undefined);
    this.farRequests.push({ method, url, body });
    const [path, query] = url.split('?');
    const hub = (hubPath: string, hubInit: RequestInit = {}) =>
      this.hub.fetch(query ? `${hubPath}?${query}` : hubPath, { ...init, ...hubInit });
    let m: RegExpExecArray | null;

    if (method === 'GET' && path === '/api/remote/chats') {
      if (this.listFails > 0) {
        this.listFails--;
        return json({ error: 'the other machine did not answer', status: 503, type: 'unavailable' }, 503);
      }
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
    if ((m = /^\/api\/remote\/attachments(\/[^/]+)?$/.exec(path))) {
      if (this.noImages) return json({ error: `no route ${method} ${path}`, status: 404, type: 'not_found' }, 404);
      return hub(`/api/attachments${m[1] ?? ''}`);
    }
    if (method === 'PUT' && (m = /^\/api\/remote\/chats\/(\d+)\/turns\/([^/]+)$/.exec(path))) {
      const { content, images } = body as { content: string; images?: string[] };
      if (this.noImages && images) {
        return json({ error: 'unknown field `images`', status: 400, type: 'invalid_request' }, 400);
      }
      // As the hub takes a device's turn: the history is its own record.
      const request = {
        conversation_id: Number(m[1]),
        replace_from: null,
        messages: [{ role: 'user', content, ...(images && { images }) }],
      } as unknown as AgentRunRequest;
      return this.hub.fetch(`/api/runs/${m[2]}?kind=agent`, { method: 'PUT', body: JSON.stringify(request) });
    }
    if (method === 'GET' && path === '/api/remote/models') {
      if (this.modelsFail > 0) {
        this.modelsFail--;
        return json({ error: 'the other machine did not answer', status: 503 }, 503);
      }
      return json(pairedModels(this.entries));
    }
    if ((m = /^\/api\/remote\/models\/([^/]+)(\/load)?$/.exec(path))) {
      const model = this.models[Number(decodeURIComponent(m[1]))];
      if (!model) {
        const error = `No model with that id or name is in the catalog: ${decodeURIComponent(m[1])}`;
        return json({ error, status: 404, type: 'model_not_found' }, 404);
      }
      if (method === 'GET' && !m[2]) return json({ detail: model } satisfies ModelLookup);
      if (method === 'POST' && m[2]) {
        model.isServing = true;
        return json({ model: model.name, started: true, context: 30_000 } satisfies LoadResponse);
      }
    }
    if (path === '/api/remote/runs') return hub('/api/runs');
    if ((m = /^\/api\/remote\/runs\/([^/]+)\/(events|cancel)$/.exec(path))) {
      return hub(`/api/runs/${m[1]}/${m[2]}`);
    }
    return json({ error: `no route ${method} ${path}`, status: 404, type: 'not_found' }, 404);
  };
}
