import { describe, it, expect, beforeEach } from 'vitest';

import {
  getAuthHeaders,
  getAuthenticatedFetchConfig,
  setApiSession,
} from '../../../../src/services/transport/api/client';

/**
 * Two ways to reach the same daemon must present the same credential.
 *
 * `getAuthenticatedFetchConfig` is what agent-chat streaming and the five
 * benchmark streams build their requests from, and it returned a literal `{}`
 * on both branches. So against a `--share-lan` daemon those calls answered 401
 * even once the user had entered the key, while every other `/api/*` request
 * in the app succeeded — a split nothing pinned, because no test covered this
 * module at all.
 */
describe('API auth headers', () => {
  beforeEach(() => {
    setApiSession('', undefined);
  });

  it('sends no Authorization header when the daemon wants no token', async () => {
    expect(getAuthHeaders()).toEqual({});

    const config = await getAuthenticatedFetchConfig();
    expect(config.headers).toEqual({});
  });

  it('sends the same header to the streaming callers as to everything else', async () => {
    setApiSession('', 'lan-daemon-key');

    expect(getAuthHeaders()).toEqual({ Authorization: 'Bearer lan-daemon-key' });

    const config = await getAuthenticatedFetchConfig();
    expect(config.headers).toEqual(getAuthHeaders());
  });

  it('drops the header again when the session is cleared', async () => {
    setApiSession('', 'lan-daemon-key');
    setApiSession('', undefined);

    const config = await getAuthenticatedFetchConfig();
    expect(config.headers).toEqual({});
  });
});
