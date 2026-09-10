/**
 * The link between the Remote panel and the request body.
 *
 * Every turn asks this one question — which machine, and under what name —
 * and the defect was that only the first half of the answer was ever asked
 * for. So both halves are pinned here, against the real registry rather than
 * a stand-in for it: the panel writes there and this reads there, and a test
 * that mocked the store in between would prove nothing about the link.
 */

import { describe, it, expect, beforeEach } from 'vitest';

import { askTheRemote } from '../../../../src/hooks/useGglibRuntime/useGglibRuntime';
import {
  IDLE_STATUS,
  applyRemoteStatus,
  getRemoteState,
  resetRemoteState,
  setRemoteChatModel,
  setUseRemoteForChat,
} from '../../../../src/services/remoteRegistry';

const connected = {
  port: 41234,
  base_url: 'http://127.0.0.1:41234/v1',
  ticket_fingerprint: '3ca82708b995',
  path: 'direct',
  away_for_s: null,
};

/** Connected, box checked, a model named — the state a remote turn needs. */
function armed(model: string): void {
  applyRemoteStatus({ ...IDLE_STATUS, connected });
  setRemoteChatModel(model);
  setUseRemoteForChat(true);
}

describe('askTheRemote', () => {
  beforeEach(() => resetRemoteState());

  it('carries the name the panel was given', () => {
    armed('qwen3');
    expect(askTheRemote()).toEqual({ remote: true, model: 'qwen3' });
  });

  it('trims what was typed', () => {
    armed('  qwen3  ');
    expect(askTheRemote().model).toBe('qwen3');
  });

  it('a blank field is no name at all, which the send path refuses', () => {
    armed('   ');
    expect(askTheRemote()).toEqual({ remote: true, model: undefined });
  });

  it('an unchecked box keeps the turn local, whatever is typed', () => {
    applyRemoteStatus({ ...IDLE_STATUS, connected });
    setRemoteChatModel('qwen3');
    expect(askTheRemote()).toEqual({ remote: false });
  });

  it('a machine that has gone takes the choice with it', () => {
    armed('qwen3');
    applyRemoteStatus(IDLE_STATUS);
    expect(askTheRemote()).toEqual({ remote: false });
  });

  // The store keeps what the panel was given, byte for byte. Trimming there
  // would swallow a space as the user types it, in a controlled input whose
  // value is the store's — so the trim lives on the send path and this is
  // what says so.
  it('the store keeps the name as typed, and the send path is what trims it', () => {
    setRemoteChatModel(' qwen3 ');
    expect(getRemoteState().chatModel).toBe(' qwen3 ');
    applyRemoteStatus({ ...IDLE_STATUS, connected });
    setUseRemoteForChat(true);
    expect(askTheRemote()).toEqual({ remote: true, model: 'qwen3' });
  });
});
