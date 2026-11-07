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

  it('holds a new chat\'s choice and writes nothing, until its first send asks for it', () => {
    const { result } = mount({ conversationId: null, conversations: [] });
    expect(result.current!.forCreate()).toBeUndefined();
    act(() => result.current!.toggle('3:search', false, LISTED));
    expect(result.current!.ticked('3:search')).toBe(false);
    expect(transport.updateConversationSettings).not.toHaveBeenCalled();

    const said = result.current!.forCreate()!;
    expect(said.patch).toStrictEqual({ tools: ['builtin:get_current_time', 'builtin:read_file'] });
    act(() => said.accepted());
    expect(result.current!.forCreate()).toBeUndefined();
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
