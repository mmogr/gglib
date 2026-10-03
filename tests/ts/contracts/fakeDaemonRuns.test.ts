/**
 * The fake daemon the chat runtime's tests run against, held to the runs
 * the real daemon sends.
 *
 * `contracts/runs/recorded.json` is written by `gglib-core`'s wire tests
 * from the real `RunInfo` serialisation, and fails there when it goes
 * stale. So a run body the fake answers with, at each status, must have the
 * keys and value types a recorded body at that status has, leave out what
 * the real one leaves out (never a `null`), and a listing must be the same
 * envelope, newest first.
 */

import { describe, it, expect } from 'vitest';

import { FakeDaemon } from '../fixtures/fakeDaemon';
import { rust } from './rustSource';

type Body = Record<string, unknown>;
const RECORDED = JSON.parse(rust('contracts/runs/recorded.json')) as Record<string, Body>;
const STATUSES = ['queued', 'in_progress', 'completed', 'failed', 'cancelled'] as const;

/** Every key a run body can carry: what any recording shows, plus the conversation. */
const ALLOWED = new Set([...STATUSES.flatMap((s) => Object.keys(RECORDED[s])), 'conversation_id']);
/** The keys every recording carries. */
const REQUIRED = Object.keys(RECORDED.queued).filter((k) => STATUSES.every((s) => k in RECORDED[s]));

const request = (content: string) => ({
  conversation_id: 1,
  replace_from: null,
  port: 9000,
  far: null,
  messages: [{ role: 'user', content }],
  config: null,
  tool_filter: null,
  model: null,
  reasoning_effort: null,
  reasoning_budget_tokens: null,
});

async function put(daemon: FakeDaemon, id: string): Promise<[number, Body]> {
  const response = await daemon.fetch(`/api/runs/${id}?kind=agent`, {
    method: 'PUT',
    body: JSON.stringify(request('hi')),
  });
  return [response.status, (await response.json()) as Body];
}

async function get(daemon: FakeDaemon, path: string): Promise<Body> {
  return (await (await daemon.fetch(path)).json()) as Body;
}

/** Run `id` as `GET /api/runs` lists it. */
async function listed(daemon: FakeDaemon, id: string): Promise<Body> {
  const runs = (await get(daemon, '/api/runs')).runs as Body[];
  return runs.find((r) => r.id === id)!;
}

/** A body at `status` has the shape a recorded body at that status has. */
function expectShaped(body: Body, status: (typeof STATUSES)[number]) {
  const recorded = RECORDED[status];
  expect(body.status).toBe(status);
  for (const key of REQUIRED) expect(body, key).toHaveProperty(key);
  for (const [key, value] of Object.entries(body)) {
    expect(ALLOWED.has(key), `a key no run carries: ${key}`).toBe(true);
    expect(value, `${key} is left out, never null`).not.toBeNull();
    if (key in recorded) expect(typeof value, key).toBe(typeof recorded[key]);
  }
  for (const key of ['finished_at_ms', 'error']) {
    expect(key in body, `${key} at ${status}`).toBe(key in recorded);
  }
}

describe('the fake daemon against the recorded runs', () => {
  it('a new run is queued, a repeated id answers 200 with the same run', async () => {
    const daemon = new FakeDaemon();
    const [created, run] = await put(daemon, 'r1');
    expect(created).toBe(201);
    expectShaped(run, 'queued');

    const [repeated, again] = await put(daemon, 'r1');
    expect(repeated).toBe(200);
    expect(again).toEqual(run);
    expect(daemon.saved(1)).toHaveLength(1);
  });

  it('a run is in progress from its first event, and each end reads as the real one', async () => {
    const daemon = new FakeDaemon();
    for (const end of ['completed', 'failed', 'cancelled'] as const) {
      await put(daemon, end);
      daemon.emit(end, { type: 'text_delta', content: 'x' });
      expectShaped(await listed(daemon, end), 'in_progress');
      await daemon.finish(end, end);
      expectShaped(await listed(daemon, end), end);
    }
  });

  it('a cancel answers the run still going, and it reads as ended once saved', async () => {
    const daemon = new FakeDaemon();
    await put(daemon, 'r1');
    daemon.emit('r1', { type: 'text_delta', content: 'x' });
    const answered = (await (await daemon.fetch('/api/runs/r1/cancel', { method: 'POST' })).json()) as Body;
    expectShaped(answered, 'in_progress');
    expect((await listed(daemon, 'r1')).status).toBe('in_progress');
    expect(daemon.saved(1)).toHaveLength(1);

    await daemon.finish('r1', 'cancelled');
    expectShaped(await listed(daemon, 'r1'), 'cancelled');
    expect(daemon.saved(1)).toHaveLength(2);
  });

  it('a listing is the recorded envelope, newest first', async () => {
    const daemon = new FakeDaemon();
    await put(daemon, 'older');
    await put(daemon, 'newer');
    const list = await get(daemon, '/api/runs');
    expect(Object.keys(list)).toEqual(Object.keys(RECORDED.list));
    expect((list.runs as Body[]).map((r) => r.id)).toEqual(['newer', 'older']);
  });
});
