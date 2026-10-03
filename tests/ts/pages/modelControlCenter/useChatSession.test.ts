/**
 * A paired chat session's machine is fixed: the session outlives that
 * machine's disconnection and a new connection that names no machine yet,
 * and is closed once the status names another machine, whose same id is
 * another model.
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { act, renderHook } from '@testing-library/react';

import { useChatSession } from '../../../../src/pages/modelControlCenter/useChatSession';
import {
  IDLE_STATUS,
  applyRemoteStatus,
  ingestRemoteEvent,
  resetRemoteState,
} from '../../../../src/services/remoteRegistry';
import type { ModelRef } from '../../../../src/types/generated/ModelRef';

const DESK = '3ca82708b995';
const far: ModelRef = { machine: { kind: 'paired', fingerprint: DESK }, id: 3 };

function connectedTo(fingerprint: string, name: string) {
  const connected = {
    port: 41234,
    base_url: 'http://127.0.0.1:41234/v1',
    ticket_fingerprint: fingerprint,
    path: 'direct' as const,
    away_for_s: null,
  };
  return { ...IDLE_STATUS, connected, paired_name: name };
}

/** The hook with a chat open on desk's model 3. */
function opened() {
  act(() => applyRemoteStatus(connectedTo(DESK, 'desk')));
  const hook = renderHook(() => useChatSession([]));
  act(() => hook.result.current.openPairedChat(far, 'qwen3', 'desk'));
  expect(hook.result.current.chatSession).toMatchObject({ kind: 'paired', far });
  return hook;
}

describe('useChatSession, paired', () => {
  beforeEach(() => resetRemoteState());

  it('closes the chat once the status names another machine', () => {
    const hook = opened();
    act(() => applyRemoteStatus(connectedTo('ffeeddccbbaa', 'study')));
    expect(hook.result.current.chatSession).toBeNull();
  });

  it('keeps the chat across a disconnection, and when that machine answers again', () => {
    const hook = opened();
    act(() => ingestRemoteEvent({ type: 'remote_disconnected' }));
    expect(hook.result.current.chatSession).toMatchObject({ kind: 'paired', far });
    act(() => applyRemoteStatus(connectedTo(DESK, 'desk')));
    expect(hook.result.current.chatSession).toMatchObject({ kind: 'paired', far });
  });

  it('keeps the chat while a new connection names no machine yet', () => {
    const hook = opened();
    act(() => ingestRemoteEvent({ type: 'remote_joined', port: 41235 }));
    expect(hook.result.current.chatSession).toMatchObject({ kind: 'paired', far });
  });
});
