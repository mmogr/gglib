/**
 * The paired machine's models as the page asks this machine's daemon for
 * them: the routes, a far model named by its id as one path segment, and
 * what comes back.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('../../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

import {
  getPairedModel,
  listPairedModels,
  loadPairedModel,
} from '../../../../src/services/transport/api/farModels';
import { FakeFarDaemon } from '../../fixtures/fakeFarDaemon';

let daemons: FakeFarDaemon;

beforeEach(() => {
  daemons = new FakeFarDaemon();
  vi.stubGlobal('fetch', vi.fn(daemons.fetch));
});
afterEach(() => {
  vi.unstubAllGlobals();
});

describe('farModels', () => {
  it('lists every entry with the machine and its actions', async () => {
    const paired = await listPairedModels();
    expect(paired.machine).toEqual({ kind: 'paired', fingerprint: '3ca82708b995' });
    expect(paired.models.map((m) => m.id)).toEqual(['qwen3', 'qwen3:coding']);
    expect(daemons.farRequests.map((r) => `${r.method} ${r.url}`)).toEqual(['GET /api/remote/models']);
  });

  it('reads and loads one model by its id there', async () => {
    expect((await getPairedModel(3)).detail.name).toBe('qwen3');
    expect((await loadPairedModel(3)).started).toBe(true);
    expect(daemons.farRequests.map((r) => `${r.method} ${r.url}`)).toEqual([
      'GET /api/remote/models/3',
      'POST /api/remote/models/3/load',
    ]);
    // The body the daemon's load route reads, as the CLI sends it.
    expect(daemons.farRequests[1].body).toEqual({ num_ctx: null });
  });

  it('a model that machine does not have is its refusal', async () => {
    await expect(getPairedModel(9)).rejects.toThrow(/in the catalog: 9/);
  });
});
