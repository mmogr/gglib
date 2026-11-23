import { describe, it, expect } from 'vitest';

import { hubLimitOf, readData, TransportError } from '../../../../src/services/transport/errors';

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
    ...init,
  });
}

describe('readData', () => {
  it('unwraps a successful ApiResponse envelope', async () => {
    const data = await readData<{ id: number }>(
      jsonResponse({ success: true, data: { id: 1 } }),
    );
    expect(data).toEqual({ id: 1 });
  });

  it('maps an HTML body to NOT_FOUND (older backend SPA fallback on missing routes)', async () => {
    const response = new Response('<!doctype html><html><body>app</body></html>', {
      status: 200,
      headers: { 'content-type': 'text/html; charset=utf-8' },
    });

    const error = await readData(response).catch((e) => e);
    expect(TransportError.hasCode(error, 'NOT_FOUND')).toBe(true);
  });

  it('maps an unparseable JSON body to a coded DECODE error, not a raw SyntaxError', async () => {
    const response = new Response('not json at all', {
      status: 200,
      headers: { 'content-type': 'application/json' },
    });

    const error = await readData(response).catch((e) => e);
    expect(error).toBeInstanceOf(TransportError);
    expect(TransportError.hasCode(error, 'DECODE')).toBe(true);
  });

  it('propagates body-level application errors without relabeling them as parse failures', async () => {
    const error = await readData(jsonResponse({ success: false, error: 'boom' })).catch(
      (e) => e,
    );
    expect(TransportError.hasCode(error, 'INTERNAL')).toBe(true);
    expect((error as TransportError).message).toBe('boom');
  });

  it("keeps the Hub's limit with a refusal of that type, for hubLimitOf to read", async () => {
    const body = {
      error: "Failed to get quantizations: Hugging Face's rate limit lifts in 2:22",
      status: 429,
      type: 'hub_rate_limited',
      metadata: { bucket: 'api', retry_after_secs: 142 },
    };
    const error = await readData(jsonResponse(body, { status: 429, statusText: 'Too Many Requests' })).catch((e) => e);
    expect(error).toBeInstanceOf(TransportError);
    expect((error as TransportError).message).toBe(body.error);
    expect((error as TransportError).details).toEqual({
      status: 429,
      statusText: 'Too Many Requests',
      type: 'hub_rate_limited',
      metadata: { bucket: 'api', retry_after_secs: 142 },
    });
    expect(hubLimitOf(error)).toEqual({ bucket: 'api', retryAfterSecs: 142 });
  });

  it('reads no Hub limit from another 429, or from one with no time to wait', async () => {
    const busy = await readData(jsonResponse({ error: 'busy', status: 429, type: 'agent_busy' }, { status: 429 })).catch((e) => e);
    expect(hubLimitOf(busy)).toBeNull();
    const untyped = await readData(jsonResponse({ error: 'slow down' }, { status: 429 })).catch((e) => e);
    expect(hubLimitOf(untyped)).toBeNull();
    const noTime = await readData(
      jsonResponse({ error: 'x', status: 429, type: 'hub_rate_limited', metadata: { bucket: 'api' } }, { status: 429 }),
    ).catch((e) => e);
    expect(hubLimitOf(noTime)).toBeNull();
    expect(hubLimitOf(new Error('plain'))).toBeNull();
  });
});
