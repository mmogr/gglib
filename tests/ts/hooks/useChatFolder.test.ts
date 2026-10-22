/**
 * The chat's folder: shown as chosen here, else as gglib remembers it;
 * said on a send only while gglib does not remember it, as the path, or
 * `""` for Clear; over once the turn that says it is accepted, after which
 * the next reading of the chat is shown. A send refused for its folder
 * leaves the chat in an error state until another folder is chosen, it is
 * cleared, or a turn is accepted. None on a far chat, and none offered to
 * a model that calls no tools unless its chat has one.
 *
 * A reading is a new list of this machine's chats: the props are rerendered
 * with one where the page would have read it.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, renderHook } from '@testing-library/react';

const platform = vi.hoisted(() => ({ desktop: false, picked: null as string | null }));
vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
  isDesktop: () => platform.desktop,
  pickFolder: vi.fn(async () => platform.picked),
}));

import { FOLDER_GONE, useChatFolder, type ChatFolderChat } from '../../../src/hooks/useChatFolder';
import { TransportError } from '../../../src/services/transport/errors';
import type { ConversationSettings } from '../../../src/services/transport';

/** This machine's list as one reading of it: each chat with what it remembers. */
function listed(...chats: Array<[number, ConversationSettings?]>) {
  return chats.map(([id, settings]) => ({ id, ...(settings && { settings }) }));
}

const chat = (props: Partial<ChatFolderChat> = {}): ChatFolderChat => ({
  conversationId: 1,
  conversations: listed([1], [2]),
  far: false,
  ...props,
});

const mount = (props: ChatFolderChat) =>
  renderHook((p: ChatFolderChat) => useChatFolder(p), { initialProps: props });

/** The daemon's refusal of a run, with its code. */
const refusal = (code: string, status: number, message: string) =>
  new TransportError(status === 409 ? 'CONFLICT' : 'VALIDATION', message, { status, type: code });

beforeEach(() => {
  platform.desktop = false;
  platform.picked = null;
});

describe('useChatFolder', () => {
  it('shows what gglib remembers, and says nothing of it on a send', () => {
    const hook = mount(chat({ conversations: listed([1, { folder: '/home/m/src/gglib' }]) }));
    expect(hook.result.current).toMatchObject({ shown: true, path: '/home/m/src/gglib', error: null });
    expect(hook.result.current.forSend()).toBeDefined();
    expect(hook.result.current.forSend()?.said).toBeUndefined();
  });

  it('says a folder chosen here on the send that changes it, and not after its turn is accepted', () => {
    const hook = mount(chat());
    expect(hook.result.current.path).toBeNull();
    act(() => hook.result.current.choose('  /home/m/notes  '));
    expect(hook.result.current.path).toBe('/home/m/notes');
    const sending = hook.result.current.forSend();
    expect(sending?.said).toBe('/home/m/notes');

    act(() => sending?.accepted(1));
    expect(hook.result.current.path).toBe('/home/m/notes');
    expect(hook.result.current.forSend()?.said).toBeUndefined();

    // The next reading is shown, the canonical path the daemon recorded.
    hook.rerender(chat({ conversations: listed([1, { folder: '/srv/notes' }]) }));
    expect(hook.result.current.path).toBe('/srv/notes');
    expect(hook.result.current.forSend()?.said).toBeUndefined();
  });

  it('says "" for Clear of a remembered folder, and nothing for Clear of none', () => {
    const hook = mount(chat({ conversations: listed([1, { folder: '/home/m/src' }], [2]) }));
    act(() => hook.result.current.clear());
    expect(hook.result.current.path).toBeNull();
    expect(hook.result.current.forSend()?.said).toBe('');

    hook.rerender(chat({ conversationId: 2, conversations: listed([1, { folder: '/home/m/src' }], [2]) }));
    act(() => hook.result.current.clear());
    expect(hook.result.current.forSend()?.said).toBeUndefined();
  });

  it('ends a choice a reading agrees with, and keeps a refused one said', () => {
    const hook = mount(chat());
    act(() => hook.result.current.choose('/home/m/notes'));
    const sending = hook.result.current.forSend();
    act(() => {
      expect(sending?.refused(new Error('offline'), 1)).toBeNull();
    });
    expect(hook.result.current.forSend()?.said).toBe('/home/m/notes');
    expect(hook.result.current.error).toBeNull();

    hook.rerender(chat({ conversations: listed([1, { folder: '/home/m/notes' }]) }));
    expect(hook.result.current.forSend()?.said).toBeUndefined();
  });

  it('holds each chat\'s choice apart', () => {
    const hook = mount(chat());
    act(() => hook.result.current.choose('/a'));
    hook.rerender(chat({ conversationId: 2 }));
    expect(hook.result.current.path).toBeNull();
    expect(hook.result.current.forSend()?.said).toBeUndefined();
    hook.rerender(chat());
    expect(hook.result.current.path).toBe('/a');
  });

  it('turns a remembered folder that is gone into the error state, which Clear ends', () => {
    const hook = mount(chat({ conversations: listed([1, { folder: '/gone' }]) }));
    let told: string | null = null;
    act(() => {
      told = hook.result.current.forSend()!.refused(refusal('folder_missing', 409, "no folder at '/gone'"), 1);
    });
    expect(told).toBe(FOLDER_GONE);
    expect(hook.result.current).toMatchObject({ path: '/gone', error: FOLDER_GONE });

    act(() => hook.result.current.clear());
    expect(hook.result.current.error).toBeNull();
    expect(hook.result.current.forSend()?.said).toBe('');
  });

  it('says why a path said was refused, in the daemon\'s words, until another is chosen', () => {
    const hook = mount(chat());
    act(() => hook.result.current.choose('notes'));
    act(() => {
      hook.result.current.forSend()!.refused(refusal('folder_unusable', 400, "'notes' is not an absolute path"), 1);
    });
    expect(hook.result.current.error).toBe(
      "This folder cannot be read: 'notes' is not an absolute path. Choose another or clear it.",
    );
    // The refused choice still stands, and is said again.
    expect(hook.result.current.forSend()?.said).toBe('notes');
    act(() => hook.result.current.choose('/home/m/notes'));
    expect(hook.result.current.error).toBeNull();
  });

  it('ends the error state when a turn is accepted', () => {
    const hook = mount(chat({ conversations: listed([1, { folder: '/gone' }]) }));
    act(() => {
      hook.result.current.forSend()!.refused(refusal('folder_missing', 409, 'gone'), 1);
    });
    act(() => hook.result.current.forSend()!.accepted(1));
    expect(hook.result.current.error).toBeNull();
  });

  it('moves a choice made before a chat existed to the chat its first send made', () => {
    // The same list: the chat is opened before it is next read.
    const conversations = listed();
    const hook = mount(chat({ conversationId: null, conversations }));
    expect(hook.result.current.shown).toBe(true);
    act(() => hook.result.current.choose('/home/m/notes'));
    const sending = hook.result.current.forSend();
    expect(sending?.said).toBe('/home/m/notes');

    act(() => sending?.accepted(9));
    hook.rerender(chat({ conversationId: 9, conversations }));
    expect(hook.result.current.path).toBe('/home/m/notes');
    expect(hook.result.current.forSend()?.said).toBeUndefined();
  });

  it('keeps a new chat\'s choice, and its error, on the chat a refused first send made', () => {
    const hook = mount(chat({ conversationId: null, conversations: [] }));
    act(() => hook.result.current.choose('/nope'));
    act(() => {
      hook.result.current.forSend()!.refused(refusal('folder_unusable', 400, "no folder at '/nope'"), 9);
    });
    hook.rerender(chat({ conversationId: 9, conversations: listed([9]) }));
    expect(hook.result.current.path).toBe('/nope');
    expect(hook.result.current.error).toMatch(/^This folder cannot be read: no folder at/);
    expect(hook.result.current.forSend()?.said).toBe('/nope');
  });

  it('is never shown on a far chat, and a far send says nothing of it', () => {
    const hook = mount(chat({ far: true, conversations: listed([1, { folder: '/home/m/src' }]) }));
    expect(hook.result.current).toMatchObject({ shown: false, path: null });
    expect(hook.result.current.forSend()).toBeUndefined();
  });

  it('is not offered to a model that calls no tools, unless the chat has a folder to clear', () => {
    const hook = mount(chat({ supportsToolCalls: false }));
    expect(hook.result.current.shown).toBe(false);
    expect(hook.result.current.forSend()).toBeUndefined();

    hook.rerender(chat({ supportsToolCalls: false, conversations: listed([1, { folder: '/home/m/src' }]) }));
    expect(hook.result.current).toMatchObject({ shown: true, path: '/home/m/src', unread: true });
  });

  it("names the paired machine a chat on its model sends what is read to, and no machine for this one's", () => {
    expect(mount(chat({ sentTo: 'desk' })).result.current).toMatchObject({ sentTo: 'desk', unread: false });
    expect(mount(chat()).result.current.sentTo).toBeNull();
  });

  it('picks with the native dialog on desktop, and changes nothing when it is cancelled', async () => {
    platform.desktop = true;
    const hook = mount(chat());
    expect(hook.result.current.native).toBe(true);
    await act(() => hook.result.current.pick());
    expect(hook.result.current.path).toBeNull();

    platform.picked = '/home/m/src/gglib';
    await act(() => hook.result.current.pick());
    expect(hook.result.current.path).toBe('/home/m/src/gglib');
    expect(hook.result.current.forSend()?.said).toBe('/home/m/src/gglib');
  });

  it('types the path on web', () => {
    expect(mount(chat()).result.current.native).toBe(false);
  });
});
