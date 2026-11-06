/**
 * The proxy registry's two kinds of fact, and the one reader that
 * reconciles them: an event moves `running` at once, the status fills in
 * what only the daemon knows, and between the two the line never shows an
 * address or a key rule the daemon has not confirmed for this run.
 */

import { describe, it, expect, beforeEach } from 'vitest';
import {
  applyProxyStatus,
  endpointFacts,
  getProxyState,
  ingestProxyEvent,
  resetProxyState,
} from '../../../src/services/proxyRegistry';
import type { ProxyStatus } from '../../../src/services/transport/types/proxy';

const UP: ProxyStatus = {
  running: true,
  state: 'on',
  port: 8080,
  base_url: 'http://192.168.1.20:8080/v1',
  key: 'required',
  pinned_model: null,
};

const OFF: ProxyStatus = { running: false, state: 'off', port: null, base_url: null, key: null, pinned_model: null };

describe('the proxy registry', () => {
  beforeEach(() => resetProxyState());

  it('takes the daemon\'s status as the whole truth: running, port and the rest', () => {
    applyProxyStatus(UP);
    expect(getProxyState()).toEqual({ running: true, port: 8080, status: UP });

    applyProxyStatus(OFF);
    expect(getProxyState()).toEqual({ running: false, port: null, status: OFF });
  });

  it('an off status with a stale port leaves no port behind', () => {
    applyProxyStatus({ ...OFF, port: 8080 });
    expect(getProxyState().port).toBeNull();
  });

  it('an event moves running and the port at once and keeps the last status for the read to replace', () => {
    applyProxyStatus(UP);
    ingestProxyEvent({ type: 'proxy_stopped' });
    expect(getProxyState()).toEqual({ running: false, port: null, status: UP });

    ingestProxyEvent({ type: 'proxy_started', port: 8081 });
    expect(getProxyState()).toEqual({ running: true, port: 8081, status: UP });
  });
});

describe('the endpoint facts', () => {
  beforeEach(() => resetProxyState());

  it('say off, with no address and no key rule, while the proxy is down', () => {
    applyProxyStatus(OFF);
    expect(endpointFacts(getProxyState())).toEqual({ state: 'off', baseUrl: null, key: null, pinnedModel: null });
  });

  it('say the daemon\'s word, address, key rule and pin once its status agrees the proxy is up', () => {
    applyProxyStatus({ ...UP, state: 'pinned', pinned_model: 'qwen3-8b' });
    expect(endpointFacts(getProxyState())).toEqual({
      state: 'pinned',
      baseUrl: 'http://192.168.1.20:8080/v1',
      key: 'required',
      pinnedModel: 'qwen3-8b',
    });
  });

  /**
   * A start event lands before the status that follows it. The address the
   * last status gave belonged to the previous run, and may have been bound
   * elsewhere, so it is not shown; the word is the event's, `on`.
   */
  it('between a start event and the read after it, say on and nothing the old status knew', () => {
    applyProxyStatus(OFF);
    ingestProxyEvent({ type: 'proxy_started', port: 8080 });
    expect(endpointFacts(getProxyState())).toEqual({ state: 'on', baseUrl: null, key: null, pinnedModel: null });
  });

  it('between a stop event and the read after it, say off, whatever the old status said', () => {
    applyProxyStatus({ ...UP, state: 'pinned', pinned_model: 'qwen3-8b' });
    ingestProxyEvent({ type: 'proxy_crashed' });
    expect(endpointFacts(getProxyState())).toEqual({ state: 'off', baseUrl: null, key: null, pinnedModel: null });
  });

  it('before any read, a start event alone says on and no more', () => {
    ingestProxyEvent({ type: 'proxy_started', port: 8080 });
    expect(endpointFacts(getProxyState())).toEqual({ state: 'on', baseUrl: null, key: null, pinnedModel: null });
  });
});
