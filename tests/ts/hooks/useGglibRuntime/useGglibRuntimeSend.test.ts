/**
 * What a send actually hands the request builder.
 *
 * `askTheRemote` is tested next door and the body is tested beside it; this
 * is the hop between them — the one the defect lived in. A turn read the
 * remote flag off the panel and left the model name behind, so both files
 * could be right and the request still arrive at the far machine as
 * `"model": ""`.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';

vi.mock('../../../../src/hooks/useGglibRuntime/streamAgentChat', () => ({
  streamAgentChat: vi.fn(async () => {}),
}));
vi.mock('../../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

import { streamAgentChat } from '../../../../src/hooks/useGglibRuntime/streamAgentChat';
import { useGglibRuntime } from '../../../../src/hooks/useGglibRuntime/useGglibRuntime';
import {
  IDLE_STATUS,
  applyRemoteStatus,
  resetRemoteState,
  setRemoteChatModel,
  setUseRemoteForChat,
} from '../../../../src/services/remoteRegistry';

/** Send one user turn through the runtime and return what was streamed. */
async function send(
  options: { selectedServerPort?: number } = { selectedServerPort: 9000 },
): Promise<Record<string, unknown>> {
  const { result } = renderHook(() => useGglibRuntime(options));
  await act(async () => {
    await result.current.runtime.thread.append({
      role: 'user',
      content: [{ type: 'text', text: 'hello' }],
    });
  });
  const mocked = vi.mocked(streamAgentChat);
  expect(mocked).toHaveBeenCalledTimes(1);
  return mocked.mock.calls[0][0] as unknown as Record<string, unknown>;
}

describe('useGglibRuntime send path', () => {
  beforeEach(() => {
    resetRemoteState();
    vi.mocked(streamAgentChat).mockClear();
  });

  it('a remote turn carries the model the panel named, not only the flag', async () => {
    applyRemoteStatus({
      ...IDLE_STATUS,
      connected: {
        port: 41234,
        base_url: 'http://127.0.0.1:41234/v1',
        ticket_fingerprint: '3ca82708b995',
        path: 'direct',
        away_for_s: null,
      },
    });
    setRemoteChatModel('qwen3');
    setUseRemoteForChat(true);

    expect(await send()).toMatchObject({ remote: true, model: 'qwen3' });
  });

  it('a remote turn sends with no local server selected at all', async () => {
    // The machine this is about serves nothing, so there is no port to pick
    // — and the guard that demanded one is what made its chat screen inert.
    // The daemon uses the tunnel's port and the stored key; the `port` in the
    // body is not consulted on that branch.
    applyRemoteStatus({
      ...IDLE_STATUS,
      connected: {
        port: 41234,
        base_url: 'http://127.0.0.1:41234/v1',
        ticket_fingerprint: '3ca82708b995',
        path: 'direct',
        away_for_s: null,
      },
    });
    setRemoteChatModel('qwen3');
    setUseRemoteForChat(true);

    const options = await send({});
    expect(options).toMatchObject({ remote: true, model: 'qwen3' });
    expect(options.selectedServerPort).toBeUndefined();
  });

  it('a local turn with no server selected never reaches the wire', async () => {
    const onError = vi.fn();
    const { result } = renderHook(() =>
      useGglibRuntime({ selectedServerPort: undefined, onError }),
    );
    await act(async () => {
      await result.current.runtime.thread.append({
        role: 'user',
        content: [{ type: 'text', text: 'hello' }],
      });
    });
    expect(streamAgentChat).not.toHaveBeenCalled();
    expect(onError).toHaveBeenCalledWith(
      expect.objectContaining({ message: 'No server selected. Please serve a model first.' }),
    );
  });

  it('a local turn names no model and lets the served one answer', async () => {
    const options = await send();
    expect(options).toMatchObject({ remote: false });
    expect(options.model).toBeUndefined();
  });
});
