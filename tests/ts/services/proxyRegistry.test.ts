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
  pinned_to: null,
};

const OFF: ProxyStatus = { running: false, state: 'off', port: null, base_url: null, key: null, pinned_model: null, pinned_to: null };

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

  it('an event moves running and the port at once and drops the status of the run before it', () => {
    applyProxyStatus(UP);
    ingestProxyEvent({ type: 'proxy_stopped' });
    expect(getProxyState()).toEqual({ running: false, port: null, status: null });

    ingestProxyEvent({ type: 'proxy_started', port: 8081 });
    expect(getProxyState()).toEqual({ running: true, port: 8081, status: null });
  });

  it('a start with no stop before it drops the status too, since it is a new run', () => {
    applyProxyStatus(UP);
    ingestProxyEvent({ type: 'proxy_started', port: 8080 });
    expect(getProxyState()).toEqual({ running: true, port: 8080, status: null });
  });

  /**
   * A stop and a start that both land before the stop's re-read: the old
   * run's status must not be shown against the new run's port, since its
   * address and key rule may both have changed.
   */
  it('a stop then a start before any read shows nothing of the old run', () => {
    applyProxyStatus({ ...UP, key: 'required' });
    ingestProxyEvent({ type: 'proxy_stopped' });
    ingestProxyEvent({ type: 'proxy_started', port: 9090 });
    expect(getProxyState().port).toBe(9090);
    expect(endpointFacts(getProxyState())).toEqual({ state: 'on', baseUrl: null, key: null, pinnedModel: null, pinnedTo: null });
  });
});

describe('the endpoint facts', () => {
  beforeEach(() => resetProxyState());

  it('say off, with no address and no key rule, while the proxy is down', () => {
    applyProxyStatus(OFF);
    expect(endpointFacts(getProxyState())).toEqual({ state: 'off', baseUrl: null, key: null, pinnedModel: null, pinnedTo: null });
  });

  it('say the daemon\'s word, address, key rule and pin once its status agrees the proxy is up', () => {
    applyProxyStatus({ ...UP, state: 'pinned', pinned_model: 'qwen3-8b', pinned_to: 'Pinned to qwen3-8b' });
    expect(endpointFacts(getProxyState())).toEqual({
      state: 'pinned',
      baseUrl: 'http://192.168.1.20:8080/v1',
      key: 'required',
      pinnedModel: 'qwen3-8b',
      pinnedTo: 'Pinned to qwen3-8b',
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
    expect(endpointFacts(getProxyState())).toEqual({ state: 'on', baseUrl: null, key: null, pinnedModel: null, pinnedTo: null });
  });

  it('between a stop event and the read after it, say off, whatever the old status said', () => {
    applyProxyStatus({ ...UP, state: 'pinned', pinned_model: 'qwen3-8b' });
    ingestProxyEvent({ type: 'proxy_crashed' });
    expect(endpointFacts(getProxyState())).toEqual({ state: 'off', baseUrl: null, key: null, pinnedModel: null, pinnedTo: null });
  });

  it('before any read, a start event alone says on and no more', () => {
    ingestProxyEvent({ type: 'proxy_started', port: 8080 });
    expect(endpointFacts(getProxyState())).toEqual({ state: 'on', baseUrl: null, key: null, pinnedModel: null, pinnedTo: null });
  });

  /**
   * A daemon older than this GUI answers without the state word, the
   * address and the key rule. The two ship together, but a dev build that
   * adopts an installed daemon does not: a missing field is "not said".
   */
  it('a status from a daemon that predates the fields says on and nothing more, not a crash', () => {
    const older = { running: true, port: 8080, pinned_model: null } as unknown as ProxyStatus;
    applyProxyStatus(older);
    expect(endpointFacts(getProxyState())).toEqual({ state: 'on', baseUrl: null, key: null, pinnedModel: null, pinnedTo: null });

    applyProxyStatus({ ...older, pinned_model: 'qwen3-8b' });
    expect(endpointFacts(getProxyState()).state).toBe('pinned');
  });
});
