/**
 * An image model's component links as the page speaks them: the pickers'
 * choices read from the model's own route, and the update's `components`
 * sent as a path to link a role, `null` to unlink it, a role left out to
 * keep its link, and no key at all to change none.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('../../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

import { listComponentChoices, updateModel } from '../../../../src/services/transport/api/models/local';
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

describe('the component links over the transport', () => {
  beforeEach(() => {
    fetchMock.mockReset();
    vi.stubGlobal('fetch', fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("reads a model's choices from its components route", async () => {
    const choices = [
      { role: 'vae', files: [{ path: '/models/u/F/ae.safetensors', name: 'ae.safetensors' }] },
      { role: 'clip_l', files: [] },
    ];
    fetchMock.mockResolvedValueOnce(json(choices));
    await expect(listComponentChoices(7)).resolves.toEqual(choices);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('/api/models/7/components');
    expect(init.method ?? 'GET').toBe('GET');
  });

  it('links and unlinks by role in one update, sending a path and a null', async () => {
    fetchMock.mockResolvedValueOnce(json(guiModel({ id: 7, imageFamily: 'flux1' })));
    await updateModel({ id: 7, components: { vae: '/models/u/F/ae.safetensors', t5xxl: null } });
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('/api/models/7');
    expect(init.method).toBe('PUT');
    expect(sentBody().components).toStrictEqual({ vae: '/models/u/F/ae.safetensors', t5xxl: null });
  });

  it('changes no component when the update does not name them', async () => {
    fetchMock.mockResolvedValueOnce(json(guiModel({ id: 7, name: 'renamed' })));
    await updateModel({ id: 7, name: 'renamed' });
    expect(sentBody()).not.toHaveProperty('components');
  });

  it("rejects a refused file with the server's sentence", async () => {
    const refusal = '/models/x/clip_l.safetensors is not a Flux.1 VAE';
    fetchMock.mockResolvedValueOnce(json({ error: refusal, status: 400 }, 400));
    await expect(updateModel({ id: 7, components: { vae: '/models/x/clip_l.safetensors' } })).rejects.toThrow(refusal);
  });
});
