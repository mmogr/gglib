import { describe, it, expect } from 'vitest';

import { readData, TransportError } from '../../../../src/services/transport/errors';

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
});
