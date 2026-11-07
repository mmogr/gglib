/**
 * The chat's tools: ticked as the chat saved them, every one where it saved
 * none and none where they are off; an entry with no `:` ticks any tool of
 * that bare name, as gglib reads the list. Each change writes only the
 * settings it changes, and the list is never left empty by an untick, since
 * an empty list is every tool. A chat not yet made writes nothing: its
 * choice is what its first send saves. None on a far chat.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';

const transport = vi.hoisted(() => ({ updateConversationSettings: vi.fn(async (_id: number, _patch: object) => {}) }));
vi.mock('../../../src/services/transport', () => ({ getTransport: () => transport }));
const registry = vi.hoisted(() => ({ refreshChats: vi.fn(async () => {}) }));
vi.mock('../../../src/services/chatRegistry', () => registry);

import {
  EVERY_TOOL,
  afterAll,
  afterToggle,
  isTicked,
  patchOf,
  useChatTools,
  type ChatToolsChat,
} from '../../../src/hooks/useChatTools';
import type { ConversationSettings, ConversationSummary } from '../../../src/services/transport';

const LISTED = ['builtin:get_current_time', 'builtin:read_file', '3:search'];

const listed = (settings?: ConversationSettings) => [{ id: 1, ...(settings && { settings }) }] as ConversationSummary[];

const mount = (props: Partial<ChatToolsChat> = {}) =>
  renderHook((p: ChatToolsChat) => useChatTools(p), {
    initialProps: { conversationId: 1, conversations: listed(), far: false, onError: vi.fn(), ...props },
  });

beforeEach(() => {
  transport.updateConversationSettings.mockReset();
  transport.updateConversationSettings.mockImplementation(async () => {});
  registry.refreshChats.mockClear();
});

describe('the rules', () => {
  it('ticks every tool where the chat names none, the ones it names, and none where they are off', () => {
    expect(LISTED.map((name) => isTicked(EVERY_TOOL, name))).toEqual([true, true, true]);
    expect(LISTED.map((name) => isTicked({ tools: ['builtin:read_file'], off: false }, name))).toEqual([false, true, false]);
    expect(LISTED.map((name) => isTicked({ tools: [], off: true }, name))).toEqual([false, false, false]);
  });

  it('ticks a tool by its bare name where the entry has no colon, never one whose entry names another server', () => {
    expect(isTicked({ tools: ['search'], off: false }, '3:search')).toBe(true);
    expect(isTicked({ tools: ['4:search'], off: false }, '3:search')).toBe(false);
    expect(isTicked({ tools: ['search'], off: false }, 'builtin:read_file')).toBe(false);
  });

  it('unticks one of every tool as the list of the others', () => {
    expect(afterToggle(EVERY_TOOL, 'builtin:read_file', false, LISTED)).toEqual({
      tools: ['builtin:get_current_time', '3:search'],
      off: false,
    });
  });

  it('ticks one onto the list it names, and back off it', () => {
    const one = { tools: ['builtin:read_file'], off: false };
    expect(afterToggle(one, '3:search', true, LISTED)).toEqual({ tools: ['builtin:read_file', '3:search'], off: false });
    expect(afterToggle({ tools: ['builtin:read_file', '3:search'], off: false }, '3:search', false, LISTED)).toEqual(one);
  });

  it('drops a bare entry that names the tool unticked', () => {
    expect(afterToggle({ tools: ['search', 'builtin:read_file'], off: false }, '3:search', false, LISTED)).toEqual({
      tools: ['builtin:read_file'],
      off: false,
    });
  });

  it('turns the tools off at the last untick, keeping the list, never leaving it empty', () => {
    expect(afterToggle({ tools: ['3:search'], off: false }, '3:search', false, LISTED)).toEqual({ tools: ['3:search'], off: true });
    expect(afterToggle(EVERY_TOOL, 'builtin:read_file', false, ['builtin:read_file'])).toEqual({ tools: [], off: true });
  });

  it('ticks one from off as that one alone', () => {
    expect(afterToggle({ tools: ['3:search'], off: true }, 'builtin:read_file', true, LISTED)).toEqual({
      tools: ['builtin:read_file'],
      off: false,
    });
  });

  it('ticks every tool of a server where the list names the server whole, and unticks one as the rest of it', () => {
    const whole = { tools: ['3:*', 'builtin:read_file'], off: false };
    const listedHere = ['builtin:read_file', '3:search', '3:extract', '4:*'];
    expect(listedHere.map((name) => isTicked(whole, name))).toEqual([true, true, true, false]);
    expect(isTicked(whole, '33:search')).toBe(false);
    expect(afterToggle(whole, '3:search', false, listedHere)).toEqual({ tools: ['3:extract', 'builtin:read_file'], off: false });
    expect(afterToggle(whole, '3:*', false, ['builtin:read_file', '3:*'])).toEqual({ tools: ['builtin:read_file'], off: false });
    // builtin:* is no server's: it names nothing.
    expect(isTicked({ tools: ['builtin:*'], off: false }, 'builtin:read_file')).toBe(false);
  });

  it("ticks a server's whole row where the chat names some of its tools, and unticks it as none of them", () => {
    // Narrowed while server 3 ran; now it is stopped, so its row is shown whole.
    const narrowed = { tools: ['builtin:read_file', '3:search', '3:extract'], off: false };
    const shown = ['builtin:read_file', '3:*'];
    expect(shown.map((name) => isTicked(narrowed, name))).toEqual([true, true]);
    expect(isTicked({ tools: ['builtin:read_file', '33:search'], off: false }, '3:*')).toBe(false);
    expect(afterToggle(narrowed, '3:*', false, shown)).toEqual({ tools: ['builtin:read_file'], off: false });
  });

  it('unticks one of every tool as every other row, each server not read named whole', () => {
    expect(afterToggle(EVERY_TOOL, 'builtin:read_file', false, ['builtin:read_file', 'builtin:get_current_time', '4:*'])).toEqual({
      tools: ['builtin:get_current_time', '4:*'],
      off: false,
    });
  });

  it('Enable all is every tool, and Disable all is off with the list kept', () => {
    expect(afterAll({ tools: ['3:search'], off: true }, true)).toEqual(EVERY_TOOL);
    expect(afterAll({ tools: ['3:search'], off: false }, false)).toEqual({ tools: ['3:search'], off: true });
  });

  it('writes only the settings a change changes', () => {
    expect(patchOf(EVERY_TOOL, { tools: ['3:search'], off: false })).toStrictEqual({ tools: ['3:search'] });
    expect(patchOf(EVERY_TOOL, { tools: [], off: true })).toStrictEqual({ no_tools: true });
    expect(patchOf({ tools: ['3:search'], off: true }, EVERY_TOOL)).toStrictEqual({ tools: [], no_tools: false });
    expect(patchOf({ tools: ['3:search'], off: true }, { tools: ['3:search'], off: false })).toStrictEqual({ no_tools: false });
    expect(patchOf(EVERY_TOOL, EVERY_TOOL)).toStrictEqual({});
  });
});

describe('useChatTools', () => {
  it('is every tool on a chat that saved none', () => {
    const { result } = mount();
    expect(result.current!.every).toBe(true);
    expect(result.current!.none).toBe(false);
    expect(LISTED.map(result.current!.ticked)).toEqual([true, true, true]);
  });

  it('writes an untick at once, only tools, shows it until the chat is read again, then shows the reading', async () => {
    const { result, rerender } = mount();
    act(() => result.current!.toggle('builtin:read_file', false, LISTED));
    expect(result.current!.ticked('builtin:read_file')).toBe(false);
    await waitFor(() => expect(transport.updateConversationSettings.mock.calls).toEqual([[1, { tools: ['builtin:get_current_time', '3:search'] }]]));
    await waitFor(() => expect(registry.refreshChats).toHaveBeenCalledWith('this'));
    // The reading now: what the daemon saved.
    rerender({ conversationId: 1, conversations: listed({ tools: ['3:search'] }), far: false, onError: vi.fn() });
    await waitFor(() => expect(LISTED.map(result.current!.ticked)).toEqual([false, false, true]));
  });

  it('writes Disable all as no_tools alone', async () => {
    const { result } = mount();
    act(() => result.current!.all(false));
    expect(result.current!.none).toBe(true);
    await waitFor(() => expect(transport.updateConversationSettings.mock.calls).toEqual([[1, { no_tools: true }]]));
  });

  it('writes nothing for a change that changes nothing', async () => {
    const { result } = mount();
    act(() => result.current!.all(true));
    await act(async () => {});
    expect(transport.updateConversationSettings).not.toHaveBeenCalled();
  });

  it('shows the reading again where the write was refused, and says why', async () => {
    const onError = vi.fn();
    transport.updateConversationSettings.mockRejectedValueOnce(new Error('Conversation not found: 1'));
    const { result } = mount({ onError });
    act(() => result.current!.toggle('3:search', false, LISTED));
    expect(result.current!.ticked('3:search')).toBe(false);
    await waitFor(() => expect(onError).toHaveBeenCalledWith("Could not save this chat's tools: Conversation not found: 1"));
    await waitFor(() => expect(result.current!.ticked('3:search')).toBe(true));
  });

  it('a write after a refused one is still written', async () => {
    const onError = vi.fn();
    transport.updateConversationSettings.mockRejectedValueOnce(new Error('Conversation not found: 1'));
    const { result } = mount({ onError });
    act(() => result.current!.toggle('3:search', false, LISTED));
    await waitFor(() => expect(onError).toHaveBeenCalled());
    act(() => result.current!.all(false));
    await waitFor(() => expect(transport.updateConversationSettings).toHaveBeenCalledTimes(2));
    expect(transport.updateConversationSettings.mock.calls[1]).toEqual([1, { no_tools: true }]);
  });

  it('holds a new chat\'s choice and writes nothing, until its first send saves it to the chat it made', async () => {
    const { result, rerender } = mount({ conversationId: null, conversations: [] });
    expect(result.current!.forCreate()).toBeUndefined();
    act(() => result.current!.toggle('3:search', false, LISTED));
    expect(result.current!.ticked('3:search')).toBe(false);
    expect(transport.updateConversationSettings).not.toHaveBeenCalled();

    const said = result.current!.forCreate()!;
    expect(said.patch).toStrictEqual({ tools: ['builtin:get_current_time', 'builtin:read_file'] });
    await act(() => said.save(7));
    expect(transport.updateConversationSettings.mock.calls).toEqual([[7, { tools: ['builtin:get_current_time', 'builtin:read_file'] }]]);
    expect(registry.refreshChats).toHaveBeenCalledWith('this');
    expect(result.current!.forCreate()).toBeUndefined();
    // A new chat after it starts from every tool again.
    rerender({ conversationId: null, conversations: [], far: false, onError: vi.fn() });
    expect(result.current!.every).toBe(true);
  });

  it('shows the first send\'s choice as the made chat\'s until it is read again, though a reading before the write says every tool', async () => {
    let land!: () => void;
    transport.updateConversationSettings.mockImplementationOnce(() => new Promise<void>((resolve) => (land = resolve)));
    const { result, rerender } = mount({ conversationId: null, conversations: [] });
    act(() => result.current!.toggle('3:search', false, LISTED));
    const said = result.current!.forCreate()!;
    let saved!: Promise<void>;
    act(() => {
      saved = said.save(7);
    });
    // The page opens the made chat, read before the write landed: it says every tool.
    rerender({ conversationId: 7, conversations: [{ id: 7 }] as ConversationSummary[], far: false, onError: vi.fn() });
    expect(result.current!.ticked('3:search')).toBe(false);
    expect(result.current!.every).toBe(false);
    // An untick now goes after the first write, from the choice, never from every tool.
    act(() => result.current!.toggle('builtin:read_file', false, LISTED));
    await waitFor(() => expect(transport.updateConversationSettings).toHaveBeenCalledTimes(1));
    await act(async () => {});
    expect(transport.updateConversationSettings).toHaveBeenCalledTimes(1);
    await act(async () => {
      land();
      await saved;
    });
    await waitFor(() => expect(transport.updateConversationSettings).toHaveBeenCalledTimes(2));
    expect(transport.updateConversationSettings.mock.calls[1]).toEqual([7, { tools: ['builtin:get_current_time'] }]);
  });

  it('drops a new chat\'s choice where its first save was refused, and rejects the save', async () => {
    transport.updateConversationSettings.mockRejectedValueOnce(new Error('Conversation not found: 7'));
    const onError = vi.fn();
    const { result, rerender } = mount({ conversationId: null, conversations: [], onError });
    act(() => result.current!.toggle('3:search', false, LISTED));
    const said = result.current!.forCreate()!;
    await act(async () => {
      await expect(said.save(7)).rejects.toThrow('Conversation not found: 7');
    });
    // The send says why; the hook does not say it twice.
    expect(onError).not.toHaveBeenCalled();
    rerender({ conversationId: null, conversations: [], far: false, onError });
    expect(result.current!.every).toBe(true);
    expect(result.current!.forCreate()).toBeUndefined();
  });

  it('writes one change after another, the last one made last, and shows it while the first lands', async () => {
    let land!: () => void;
    let landSecond!: () => void;
    transport.updateConversationSettings
      .mockImplementationOnce(() => new Promise<void>((resolve) => (land = resolve)))
      .mockImplementationOnce(() => new Promise<void>((resolve) => (landSecond = resolve)));
    const { result } = mount();
    act(() => result.current!.toggle('3:search', false, LISTED));
    act(() => result.current!.toggle('builtin:read_file', false, LISTED));
    // The second waits for the first.
    await waitFor(() => expect(transport.updateConversationSettings).toHaveBeenCalledTimes(1));
    await act(async () => {});
    expect(transport.updateConversationSettings).toHaveBeenCalledTimes(1);
    expect(LISTED.map(result.current!.ticked)).toEqual([true, false, false]);
    await act(async () => land());
    await waitFor(() => expect(transport.updateConversationSettings).toHaveBeenCalledTimes(2));
    expect(transport.updateConversationSettings.mock.calls).toEqual([
      [1, { tools: ['builtin:get_current_time', 'builtin:read_file'] }],
      [1, { tools: ['builtin:get_current_time'] }],
    ]);
    // The first write landing does not drop the second choice, still on its way.
    expect(LISTED.map(result.current!.ticked)).toEqual([true, false, false]);
    await act(async () => landSecond());
  });

  it('asks nothing of an open chat at a send, even one a choice was made for before it was: its changes are written as they are made', () => {
    const { result, rerender } = mount({ conversationId: null, conversations: [] });
    act(() => result.current!.toggle('3:search', false, LISTED));
    rerender({ conversationId: 1, conversations: listed(), far: false, onError: vi.fn() });
    expect(result.current!.forCreate()).toBeUndefined();
    act(() => result.current!.toggle('builtin:read_file', false, LISTED));
    expect(result.current!.forCreate()).toBeUndefined();
  });

  it('is none on a far chat', () => {
    expect(mount({ far: true }).result.current).toBeUndefined();
  });
});
