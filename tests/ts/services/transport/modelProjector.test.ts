/**
 * The projector link as the page speaks it: the picker's choices read from
 * the model's own route, and the update's `projectorPath` sent as a path to
 * link, `null` to unlink, and no key at all to leave the link alone.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('../../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

import { listProjectorChoices, updateModel } from '../../../../src/services/transport/api/models/local';
import { guiModel } from '../../fixtures/model';

const fetchMock = vi.fn();

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

/** The body of the one request made. */
function sentBody(): Record<string, unknown> {
  const [, init] = fetchMock.mock.calls[0] as [string, RequestInit];
  return JSON.parse(init.body as string) as Record<string, unknown>;
}

describe('the projector link over the transport', () => {
  beforeEach(() => {
    fetchMock.mockReset();
    vi.stubGlobal('fetch', fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("reads a model's choices from its projectors route", async () => {
    const choices = [{ path: '/models/x/mmproj-Q8_0.gguf', name: 'mmproj-Q8_0.gguf' }];
    fetchMock.mockResolvedValueOnce(json(choices));
    await expect(listProjectorChoices(7)).resolves.toEqual(choices);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('/api/models/7/projectors');
    expect(init.method ?? 'GET').toBe('GET');
  });

  it('links by sending the path', async () => {
    fetchMock.mockResolvedValueOnce(json(guiModel({ id: 7, imageInput: true })));
    await updateModel({ id: 7, projectorPath: '/models/x/mmproj-Q8_0.gguf' });
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('/api/models/7');
    expect(init.method).toBe('PUT');
    expect(sentBody().projectorPath).toBe('/models/x/mmproj-Q8_0.gguf');
  });

  it('unlinks by sending null, which is not the same as sending nothing', async () => {
    fetchMock.mockResolvedValueOnce(json(guiModel({ id: 7 })));
    await updateModel({ id: 7, projectorPath: null });
    expect(sentBody()).toHaveProperty('projectorPath', null);
  });

  it('leaves the link alone when the update does not name it', async () => {
    fetchMock.mockResolvedValueOnce(json(guiModel({ id: 7, name: 'renamed' })));
    await updateModel({ id: 7, name: 'renamed' });
    expect(sentBody()).not.toHaveProperty('projectorPath');
  });

  it("rejects a refused file with the server's sentence", async () => {
    fetchMock.mockResolvedValueOnce(
      json({ error: '/models/x/X.Q8_0.gguf holds model weights, not a projector', status: 400 }, 400),
    );
    await expect(updateModel({ id: 7, projectorPath: '/models/x/X.Q8_0.gguf' })).rejects.toThrow(
      '/models/x/X.Q8_0.gguf holds model weights, not a projector',
    );
  });
});
