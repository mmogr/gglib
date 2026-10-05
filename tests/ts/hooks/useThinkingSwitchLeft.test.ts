/**
 * `useThinkingSwitch`, for a turn accepted after its chat was left.
 *
 * A far machine can answer a turn after the page has moved on to another of
 * its chats, when no reading of the first chat is held any more. The choice
 * that turn carried must still end there: held on, it would be said again
 * when the chat is next opened, over whatever another device chose since.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';

vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

import { FAR_FINGERPRINT, FakeFarDaemon, farEntry } from '../fixtures/fakeFarDaemon';
import { useThinkingSwitch, type ThinkingSwitchChat } from '../../../src/hooks/useThinkingSwitch';
import { IDLE_STATUS, applyRemoteStatus, resetRemoteState } from '../../../src/services/remoteRegistry';
import type { ConversationSettings } from '../../../src/services/transport';
import type { HubChatOpen } from '../../../src/types/generated/HubChatOpen';

const connected = {
  port: 41234,
  base_url: 'http://127.0.0.1:41234/v1',
  ticket_fingerprint: FAR_FINGERPRINT,
  path: 'direct',
  away_for_s: null,
};

let daemons: FakeFarDaemon;

beforeEach(() => {
  daemons = new FakeFarDaemon();
  vi.stubGlobal('fetch', vi.fn(daemons.fetch));
  resetRemoteState();
});
afterEach(() => {
  vi.unstubAllGlobals();
});

function farChat(id: number, settings?: ConversationSettings): HubChatOpen {
  return {
    conversation: { id, title: 'Far', model_id: null, system_prompt: null, created_at: '', updated_at: '', ...(settings && { settings }) },
    messages: [],
  };
}

const far = (conversationId: number, farOpen: HubChatOpen | null): ThinkingSwitchChat => ({
  conversationId,
  conversations: [],
  far: true,
  farOpen,
  thinks: true,
});

describe('useThinkingSwitch, a turn accepted after its chat was left', () => {
  it('ends the choice with no reading of that chat held, and shows what the chat is next read as remembering', async () => {
    daemons.entries = [farEntry('thinker', 5, { capabilities: ['reasoning'] })];
    act(() => applyRemoteStatus({ ...IDLE_STATUS, connected, paired_name: 'desk' }));
    const hook = renderHook((props: ThinkingSwitchChat) => useThinkingSwitch(props), {
      initialProps: far(1, farChat(1, { model_name: 'thinker' })),
    });
    await waitFor(() => expect(hook.result.current.shown).toBe(true));
    act(() => hook.result.current.toggle());
    const sending = hook.result.current.forSend();
    expect(sending?.said).toBe('off');

    // The second chat is opened and answered before the first chat's turn is.
    const second = farChat(2, { model_name: 'thinker' });
    hook.rerender(far(2, second));
    act(() => sending?.accepted());
    expect(hook.result.current.on).toBe(true);
    expect(hook.result.current.forSend()).toBeUndefined();

    // Back on the first chat: no switch until it is read, and nothing said.
    hook.rerender(far(1, second));
    expect(hook.result.current.shown).toBe(false);
    expect(hook.result.current.forSend()).toBeUndefined();

    // Read as remembering nothing (another device switched it back on).
    hook.rerender(far(1, farChat(1, { model_name: 'thinker' })));
    expect(hook.result.current.on).toBe(true);
    expect(hook.result.current.forSend()).toBeUndefined();

    // Read as remembering Off (nobody else touched it): off, and nothing said.
    hook.rerender(far(1, farChat(1, { model_name: 'thinker', thinking: 'off' })));
    expect(hook.result.current.on).toBe(false);
    expect(hook.result.current.forSend()).toBeUndefined();
  });
});
