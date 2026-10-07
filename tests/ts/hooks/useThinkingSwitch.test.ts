/**
 * The chat's Thinking switch: shown only where the chat's model thinks, by
 * one rule (`thinks`) for each kind of chat; showing what gglib remembers of
 * the chat until it is clicked; and saying the choice on a send only while
 * gglib does not yet remember it. A choice ends when the turn that says it is
 * accepted, after which it shows only until the chat is next read, or when a
 * reading of the chat agrees with it before such a turn is. So a chat
 * changed on another device shows that change, also after it was left with
 * its reply still being written, and one chat's choice is never another's.
 *
 * A reading is a new list (this machine's chats) or a new opened chat (a far
 * one): the props here are rerendered with one where the page would have
 * read it, and kept the same object where it would not. A send is `forSend`
 * asked, and its turn is accepted when the `accepted` of that answer is
 * called, as the runtime does once the daemon has taken the turn.
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
import type { Message } from '../../../src/types/generated/Message';
import type { ModelRef } from '../../../src/types/generated/ModelRef';

const paired: ModelRef = { machine: { kind: 'paired', fingerprint: FAR_FINGERPRINT }, id: 3 };
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

function join() {
  act(() => applyRemoteStatus({ ...IDLE_STATUS, connected, paired_name: 'desk' }));
}

/** This machine's list as one reading of it: each chat with what it remembers. */
function listed(...chats: Array<[number, ConversationSettings?]>) {
  return chats.map(([id, settings]) => ({ id, ...(settings && { settings }) }));
}

/** A far chat as its machine answers it. */
function farChat(id: number, settings?: ConversationSettings, messages: Message[] = []): HubChatOpen {
  return {
    conversation: { id, title: 'Far', model_id: null, system_prompt: null, created_at: '', updated_at: '', ...(settings && { settings }) },
    messages,
  };
}

function reply(id: number, metadata?: Record<string, unknown>): Message {
  return { id, conversation_id: 1, role: 'assistant', content: 'Answered.', created_at: '', ...(metadata && { metadata }) };
}

/** A chat on this machine's model, which thinks unless said otherwise. */
const local = (chat: Partial<ThinkingSwitchChat> = {}): ThinkingSwitchChat => ({
  conversationId: 1,
  conversations: listed([1], [2]),
  far: false,
  farOpen: null,
  thinks: true,
  ...chat,
});

const mount = (chat: ThinkingSwitchChat) =>
  renderHook((props: ThinkingSwitchChat) => useThinkingSwitch(props), { initialProps: chat });

/** A send whose turn the daemon accepts: what it said of the switch. */
function sendAccepted(hook: ReturnType<typeof mount>) {
  const sending = hook.result.current.forSend();
  act(() => sending?.accepted());
  return sending?.said;
}

/** The paired machine lists its models again, as it does when the window is focused. */
async function relist(entries: FakeFarDaemon['entries']) {
  const before = daemons.farCount('GET', '/api/remote/models');
  daemons.entries = entries;
  act(() => {
    window.dispatchEvent(new Event('focus'));
  });
  await waitFor(() => expect(daemons.farCount('GET', '/api/remote/models')).toBe(before + 1));
}

describe('useThinkingSwitch, where it is shown', () => {
  it("shows for this machine's model that thinks, and not for one that does not", () => {
    const hook = mount(local({ thinks: true }));
    expect(hook.result.current.shown).toBe(true);
    hook.rerender(local({ thinks: false }));
    expect(hook.result.current.shown).toBe(false);
  });

  it('shows nothing with no chat open', () => {
    const hook = mount(local());
    expect(hook.result.current.shown).toBe(true);
    hook.rerender(local({ conversationId: null }));
    expect(hook.result.current.shown).toBe(false);
  });

  it("reads this machine's model for a local chat and never asks the paired machine", async () => {
    join();
    const hook = mount(local());
    await act(async () => {
      await new Promise((r) => setTimeout(r, 10));
    });
    expect(daemons.farCount('GET', '/api/remote/models')).toBe(0);
    // The same hook does ask once the chat is on that machine's model.
    hook.rerender(local({ paired }));
    await waitFor(() => expect(daemons.farCount('GET', '/api/remote/models')).toBe(1));
  });

  it("judges the paired machine's model by the row that machine lists for it, not by this machine's model", async () => {
    daemons.entries = [farEntry('qwen3', 3, { capabilities: ['vision', 'reasoning'] })];
    join();
    const hook = mount(local({ paired, thinks: false }));
    expect(hook.result.current.shown).toBe(false);
    await waitFor(() => expect(hook.result.current.shown).toBe(true));

    await relist([farEntry('qwen3', 3, { capabilities: ['vision'] })]);
    await waitFor(() => expect(hook.result.current.shown).toBe(false));
    // This machine's model thinking changes nothing for a chat on the other's.
    hook.rerender(local({ paired, thinks: true }));
    expect(hook.result.current.shown).toBe(false);
  });

  it('shows nothing for a paired model that machine does not list, or once it is no longer connected', async () => {
    daemons.entries = [farEntry('qwen3', 3, { capabilities: ['reasoning'] }), farEntry('other', 4, { capabilities: ['reasoning'] })];
    join();
    const hook = mount(local({ paired, thinks: true }));
    await waitFor(() => expect(hook.result.current.shown).toBe(true));

    hook.rerender(local({ paired: { ...paired, id: 8 }, thinks: true }));
    expect(hook.result.current.shown).toBe(false);

    hook.rerender(local({ paired, thinks: true }));
    expect(hook.result.current.shown).toBe(true);
    act(() => resetRemoteState());
    expect(hook.result.current.shown).toBe(false);
  });

  it("judges a far chat by the row listed under the name the chat last ran on, else its last reply's", async () => {
    daemons.entries = [farEntry('thinker', 5, { capabilities: ['reasoning'] }), farEntry('plain', 6)];
    join();
    const far = (farOpen: HubChatOpen) => local({ far: true, conversations: [], thinks: false, farOpen });

    const hook = mount(far(farChat(1, { model_name: 'thinker' }, [reply(2, { modelName: 'plain' })])));
    await waitFor(() => expect(hook.result.current.shown).toBe(true));
    // Its settings name the model before any reply does.
    hook.rerender(far(farChat(1, { model_name: 'plain' }, [reply(2, { modelName: 'thinker' })])));
    expect(hook.result.current.shown).toBe(false);

    // With no model in its settings, the newest reply that names one decides.
    hook.rerender(far(farChat(1, undefined, [reply(2, { modelName: 'plain' }), reply(3, { modelName: 'thinker' }), reply(4)])));
    expect(hook.result.current.shown).toBe(true);
    hook.rerender(far(farChat(1, undefined, [reply(2, { modelName: 'thinker' }), reply(3, { modelName: 'plain' })])));
    expect(hook.result.current.shown).toBe(false);
    // A turn of the user's names no model, whatever its row carries.
    hook.rerender(far(farChat(1, undefined, [reply(2, { modelName: 'plain' }), { ...reply(3, { modelName: 'thinker' }), role: 'user' }])));
    expect(hook.result.current.shown).toBe(false);
  });

  it('shows nothing for a far chat that has never run, one whose model is not listed, or one not yet read', async () => {
    daemons.entries = [farEntry('thinker', 5, { capabilities: ['reasoning'] })];
    join();
    const far = (farOpen: HubChatOpen | null) => local({ far: true, conversations: [], thinks: true, farOpen });
    const hook = mount(far(farChat(1, { model_name: 'thinker' })));
    await waitFor(() => expect(hook.result.current.shown).toBe(true));

    hook.rerender(far(farChat(1)));
    expect(hook.result.current.shown).toBe(false);
    hook.rerender(far(farChat(1, { model_name: 'gone' })));
    expect(hook.result.current.shown).toBe(false);
    hook.rerender(far(null));
    expect(hook.result.current.shown).toBe(false);
    // Another chat's reading says nothing of the one open.
    hook.rerender(far(farChat(9, { model_name: 'thinker' })));
    expect(hook.result.current.shown).toBe(false);
    hook.rerender(far(farChat(1, { model_name: 'thinker' })));
    expect(hook.result.current.shown).toBe(true);
  });

  it('shows nothing for a far chat on a machine whose gglib lists no model as thinking, and says nothing to it', async () => {
    daemons.entries = [farEntry('thinker', 5, { capabilities: ['reasoning'] })];
    join();
    const hook = mount(local({ far: true, conversations: [], thinks: true, farOpen: farChat(1, { model_name: 'thinker' }) }));
    await waitFor(() => expect(hook.result.current.shown).toBe(true));
    act(() => hook.result.current.toggle());
    expect(hook.result.current.forSend()?.said).toBe('off');

    // That machine's gglib as one from before the choice lists it: a key it
    // does not know would be refused, so none is sent.
    await relist([farEntry('thinker', 5, { capabilities: ['vision'] })]);
    await waitFor(() => expect(hook.result.current.shown).toBe(false));
    expect(hook.result.current.forSend()).toBeUndefined();
  });
});

describe('useThinkingSwitch, what it shows and says', () => {
  it('opens on, and says nothing, for a chat that remembers nothing', () => {
    const { result } = mount(local());
    expect(result.current.on).toBe(true);
    expect(result.current.forSend()).toBeUndefined();
  });

  it('opens off for a chat that remembers Off, and says nothing: gglib already knows', () => {
    const { result } = mount(local({ conversations: listed([1, { thinking: 'off' }], [2]) }));
    expect(result.current.on).toBe(false);
    expect(result.current.forSend()).toBeUndefined();
  });

  it('opens a far chat as its machine remembers it', async () => {
    daemons.entries = [farEntry('thinker', 5, { capabilities: ['reasoning'] })];
    join();
    const off = mount(local({ far: true, conversations: [], farOpen: farChat(1, { model_name: 'thinker', thinking: 'off' }) }));
    await waitFor(() => expect(off.result.current.shown).toBe(true));
    expect(off.result.current.on).toBe(false);
    expect(off.result.current.forSend()).toBeUndefined();
  });

  it('switched off, says off at every send until one is accepted, then nothing', () => {
    const hook = mount(local());
    act(() => hook.result.current.toggle());
    expect(hook.result.current.on).toBe(false);
    expect(hook.result.current.forSend()?.said).toBe('off');
    // A send gglib refused changed nothing there: the next one says it again.
    expect(hook.result.current.forSend()?.said).toBe('off');

    // A reading that still remembers nothing: the choice stands.
    hook.rerender(local({ conversations: listed([1], [2]) }));
    expect(hook.result.current.forSend()?.said).toBe('off');

    // This send is accepted, and the list read after its reply says what gglib remembers.
    expect(sendAccepted(hook)).toBe('off');
    expect(hook.result.current.forSend()).toBeUndefined();
    hook.rerender(local({ conversations: listed([1, { thinking: 'off' }], [2]) }));
    expect(hook.result.current.on).toBe(false);
    expect(hook.result.current.forSend()).toBeUndefined();
  });

  it('switched back on, says default once to a chat that remembers Off, and nothing once it has forgotten', () => {
    const off = listed([1, { thinking: 'off' }]);
    const hook = mount(local({ conversations: off }));
    act(() => hook.result.current.toggle());
    expect(hook.result.current.on).toBe(true);
    expect(sendAccepted(hook)).toBe('default');
    expect(hook.result.current.on).toBe(true);
    expect(hook.result.current.forSend()).toBeUndefined();

    hook.rerender(local({ conversations: listed([1]) }));
    expect(hook.result.current.on).toBe(true);
    expect(hook.result.current.forSend()).toBeUndefined();
  });

  it('a choice a reading agreed with before any turn said it is over: a later change on another device is shown, and nothing is said against it', () => {
    const hook = mount(local());
    act(() => hook.result.current.toggle());
    // The phone switched the chat off as well, and the list read here says so.
    hook.rerender(local({ conversations: listed([1, { thinking: 'off' }]) }));
    expect(hook.result.current.on).toBe(false);
    expect(hook.result.current.forSend()).toBeUndefined();

    // The phone switches it back on; the page reads the list again.
    hook.rerender(local({ conversations: listed([1]) }));
    expect(hook.result.current.on).toBe(true);
    expect(hook.result.current.forSend()).toBeUndefined();
  });

  it('shows a change made on another device at the next reading, with nothing chosen here', () => {
    const hook = mount(local());
    hook.rerender(local({ conversations: listed([1, { thinking: 'off' }]) }));
    expect(hook.result.current.on).toBe(false);
    expect(hook.result.current.forSend()).toBeUndefined();
  });

  it('a switch back during the reply that said off is kept: the next send says default', () => {
    const before = listed([1], [2]);
    const hook = mount(local({ conversations: before }));
    act(() => hook.result.current.toggle());
    expect(sendAccepted(hook)).toBe('off');
    // The reply is being written; the list has not been read since. Switched back on:
    // gglib remembers Off from the turn it accepted, whatever the list held here says.
    act(() => hook.result.current.toggle());
    expect(hook.result.current.on).toBe(true);
    expect(hook.result.current.forSend()?.said).toBe('default');

    // The reply ends and the list says what its send made gglib remember.
    hook.rerender(local({ conversations: listed([1, { thinking: 'off' }], [2]) }));
    expect(hook.result.current.on).toBe(true);
    expect(sendAccepted(hook)).toBe('default');

    hook.rerender(local({ conversations: listed([1], [2]) }));
    expect(hook.result.current.on).toBe(true);
    expect(hook.result.current.forSend()).toBeUndefined();
  });

  it('a switch made while a reply is written is said by the next send', () => {
    const hook = mount(local());
    // Nothing chosen when the reply's send went.
    expect(hook.result.current.forSend()).toBeUndefined();
    act(() => hook.result.current.toggle());
    // The reply ends; the list still remembers nothing.
    hook.rerender(local({ conversations: listed([1], [2]) }));
    expect(hook.result.current.forSend()?.said).toBe('off');
  });

  it("one chat's switch never moves another's", () => {
    const chats = listed([1], [2, { thinking: 'off' }]);
    const hook = mount(local({ conversations: chats }));
    act(() => hook.result.current.toggle());
    expect(hook.result.current.forSend()?.said).toBe('off');

    hook.rerender(local({ conversations: chats, conversationId: 2 }));
    expect(hook.result.current.on).toBe(false);
    expect(hook.result.current.forSend()).toBeUndefined();
    act(() => hook.result.current.toggle());
    expect(hook.result.current.on).toBe(true);
    expect(hook.result.current.forSend()?.said).toBe('default');

    hook.rerender(local({ conversations: chats, conversationId: 1 }));
    expect(hook.result.current.on).toBe(false);
    expect(hook.result.current.forSend()?.said).toBe('off');
  });

  it("a far chat's id names another chat here: neither's switch moves the other's", async () => {
    daemons.entries = [farEntry('thinker', 5, { capabilities: ['reasoning'] })];
    join();
    // Chat 1 here remembers nothing; chat 1 there remembers Off.
    const here = local();
    const there = local({ far: true, conversations: [], farOpen: farChat(1, { model_name: 'thinker', thinking: 'off' }) });
    const hook = mount(here);
    act(() => hook.result.current.toggle());
    expect(hook.result.current.forSend()?.said).toBe('off');

    hook.rerender(there);
    await waitFor(() => expect(hook.result.current.shown).toBe(true));
    // Off there by that machine's own memory: the click made here is not said to it.
    expect(hook.result.current.on).toBe(false);
    expect(hook.result.current.forSend()).toBeUndefined();
    act(() => hook.result.current.toggle());
    expect(hook.result.current.on).toBe(true);
    expect(hook.result.current.forSend()?.said).toBe('default');

    // Nor did that machine's answer, which agrees with the click here, end it.
    hook.rerender(here);
    expect(hook.result.current.on).toBe(false);
    expect(hook.result.current.forSend()?.said).toBe('off');

    hook.rerender(there);
    expect(hook.result.current.on).toBe(true);
    expect(hook.result.current.forSend()?.said).toBe('default');
  });

  it('a far chat says its choice until its machine answers that it remembers it', async () => {
    daemons.entries = [farEntry('thinker', 5, { capabilities: ['reasoning'] })];
    join();
    const far = (settings: ConversationSettings) => local({ far: true, conversations: [], farOpen: farChat(1, settings) });
    const hook = mount(far({ model_name: 'thinker' }));
    await waitFor(() => expect(hook.result.current.shown).toBe(true));
    act(() => hook.result.current.toggle());
    expect(hook.result.current.forSend()?.said).toBe('off');

    // Read again after a send that machine refused: nothing changed there.
    hook.rerender(far({ model_name: 'thinker' }));
    expect(hook.result.current.forSend()?.said).toBe('off');

    hook.rerender(far({ model_name: 'thinker', thinking: 'off' }));
    expect(hook.result.current.on).toBe(false);
    expect(hook.result.current.forSend()).toBeUndefined();

    // That choice is over: switched back on from a phone, the chat shows on, and nothing is said against it.
    hook.rerender(far({ model_name: 'thinker' }));
    expect(hook.result.current.on).toBe(true);
    expect(hook.result.current.forSend()).toBeUndefined();
  });

  it('says nothing for a model that does not think, whatever was chosen', () => {
    const hook = mount(local());
    act(() => hook.result.current.toggle());
    expect(hook.result.current.forSend()?.said).toBe('off');
    hook.rerender(local({ thinks: false }));
    expect(hook.result.current.shown).toBe(false);
    expect(hook.result.current.forSend()).toBeUndefined();
  });

  it('gives the send one function for the life of the page, which answers as of when it is asked', () => {
    const hook = mount(local());
    const { forSend } = hook.result.current;
    expect(forSend()).toBeUndefined();
    act(() => hook.result.current.toggle());
    expect(hook.result.current.forSend).toBe(forSend);
    expect(forSend()?.said).toBe('off');
  });
});

describe('useThinkingSwitch, how a choice ends', () => {
  it('is said until its turn is accepted, then shown and not said until the chat is next read, then the reading is shown whatever it says', () => {
    const held = listed([1], [2]);
    const hook = mount(local({ conversations: held }));
    act(() => hook.result.current.toggle());

    // Never sent: shown, and said by a send.
    expect(hook.result.current.on).toBe(false);
    expect(hook.result.current.forSend()?.said).toBe('off');

    // Its turn accepted: gglib remembers it, so no send says it; it still
    // shows, though the list held here is the one read before it.
    expect(sendAccepted(hook)).toBe('off');
    expect(hook.result.current.on).toBe(false);
    expect(hook.result.current.forSend()).toBeUndefined();
    hook.rerender(local({ conversations: held }));
    expect(hook.result.current.on).toBe(false);
    expect(hook.result.current.forSend()).toBeUndefined();

    // The chat is read again and remembers nothing, switched back on from a
    // phone: the reading is what shows, and nothing is said against it.
    hook.rerender(local({ conversations: listed([1], [2]) }));
    expect(hook.result.current.on).toBe(true);
    expect(hook.result.current.forSend()).toBeUndefined();
    // Nor does the choice come back with a later reading that would agree with it.
    hook.rerender(local({ conversations: listed([1, { thinking: 'off' }], [2]) }));
    hook.rerender(local({ conversations: listed([1], [2]) }));
    expect(hook.result.current.on).toBe(true);
    expect(hook.result.current.forSend()).toBeUndefined();
  });

  it.each([
    ['on', 'default', { thinking: 'off' }],
    ['off', 'off', {}],
  ] as const)(
    'a far chat switched %s here, left before its reply ended and switched back from a phone, opens as the phone left it',
    async (_, said, remembers) => {
      daemons.entries = [farEntry('thinker', 5, { capabilities: ['reasoning'] })];
      join();
      const far = (conversationId: number, farOpen: HubChatOpen) => local({ far: true, conversations: [], conversationId, farOpen });
      const hook = mount(far(1, farChat(1, { model_name: 'thinker', ...remembers })));
      await waitFor(() => expect(hook.result.current.shown).toBe(true));
      const before = hook.result.current.on;
      expect(before).toBe(said === 'off');
      act(() => hook.result.current.toggle());

      expect(sendAccepted(hook)).toBe(said);
      expect(hook.result.current.on).toBe(!before);
      expect(hook.result.current.forSend()).toBeUndefined();

      // Another far chat is opened while the reply is written: the reading
      // after that reply is never made. A phone's turn switches chat 1 back.
      const other = farChat(2, { model_name: 'thinker' });
      hook.rerender(far(2, other));
      expect(hook.result.current.on).toBe(true);
      // Chat 1 is opened again: nothing shows until its machine answers.
      hook.rerender(far(1, other));
      expect(hook.result.current.shown).toBe(false);
      expect(hook.result.current.forSend()).toBeUndefined();
      hook.rerender(far(1, farChat(1, { model_name: 'thinker', ...remembers })));
      expect(hook.result.current.shown).toBe(true);
      expect(hook.result.current.on).toBe(before);
      expect(hook.result.current.forSend()).toBeUndefined();
    },
  );

  it('an accepted turn ends the choice of the chat it was sent in, whichever chat is open when it is accepted', () => {
    const held = listed([1], [2]);
    const hook = mount(local({ conversations: held }));
    act(() => hook.result.current.toggle());
    const sending = hook.result.current.forSend();
    expect(sending?.said).toBe('off');

    // Chat 2 is opened before the daemon answers the send.
    hook.rerender(local({ conversations: held, conversationId: 2 }));
    act(() => sending?.accepted());
    expect(hook.result.current.on).toBe(true);
    expect(hook.result.current.forSend()).toBeUndefined();

    hook.rerender(local({ conversations: held, conversationId: 1 }));
    expect(hook.result.current.on).toBe(false);
    expect(hook.result.current.forSend()).toBeUndefined();
  });

  it('a switch back before the turn is accepted is kept: gglib remembers what the turn said, and the next send says the other', () => {
    const hook = mount(local());
    act(() => hook.result.current.toggle());
    const sending = hook.result.current.forSend();
    expect(sending?.said).toBe('off');
    act(() => hook.result.current.toggle());
    act(() => sending?.accepted());
    expect(hook.result.current.on).toBe(true);
    expect(hook.result.current.forSend()?.said).toBe('default');
  });

  it("this machine's list ends every choice it agrees with, not only the open chat's", () => {
    const hook = mount(local({ conversations: listed([1], [2]) }));
    act(() => hook.result.current.toggle());
    expect(hook.result.current.forSend()?.said).toBe('off');
    // Chat 2 is opened, and the list read then says chat 1 remembers Off: a phone switched it off too.
    hook.rerender(local({ conversationId: 2, conversations: listed([1, { thinking: 'off' }], [2]) }));
    // The phone switches chat 1 back on; chat 1 is opened and the list read again.
    hook.rerender(local({ conversationId: 1, conversations: listed([1], [2]) }));
    expect(hook.result.current.on).toBe(true);
    expect(hook.result.current.forSend()).toBeUndefined();
  });

  it('an accepted choice shows only until the list is next read, whichever chat is open when it is', () => {
    const hook = mount(local({ conversations: listed([1], [2]) }));
    act(() => hook.result.current.toggle());
    expect(sendAccepted(hook)).toBe('off');
    // Chat 2 is opened while the reply is written, and the list read then: a
    // phone has already switched chat 1 back on.
    const read = listed([1], [2]);
    hook.rerender(local({ conversationId: 2, conversations: read }));
    // Chat 1 is opened again, the list not yet read again.
    hook.rerender(local({ conversationId: 1, conversations: read }));
    expect(hook.result.current.on).toBe(true);
    expect(hook.result.current.forSend()).toBeUndefined();
  });

  it("the far machine's list, which says nothing of a chat's settings, ends no choice made on this machine's chat of the same id", () => {
    // Chat 1 here remembers Off and is switched on here: not yet sent.
    const here = local({ conversations: listed([1, { thinking: 'off' }]) });
    const hook = mount(here);
    act(() => hook.result.current.toggle());
    expect(hook.result.current.on).toBe(true);
    expect(hook.result.current.forSend()?.said).toBe('default');

    // The rail moves to the far machine's chats: the page's list is now that
    // machine's summaries, each with no settings, as the page hands them on.
    hook.rerender(local({ far: true, conversations: listed([1], [2]), farOpen: null }));
    // Back to this machine's chats.
    hook.rerender(here);
    expect(hook.result.current.on).toBe(true);
    expect(hook.result.current.forSend()?.said).toBe('default');
  });
});
