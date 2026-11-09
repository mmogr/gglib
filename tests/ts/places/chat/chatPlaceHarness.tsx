/**
 * What the chat place tests share: a transport stub the test shapes, the
 * providers the place needs, and the jsdom gaps its tree falls into. The
 * chat list is a module store, so each test starts with it unread.
 *
 * Each test file mocks the transport module itself (vi.mock is hoisted per
 * file) and hands `getTransport` whatever `chatTransport` built.
 */

import { beforeEach, vi } from 'vitest';
import { fireEvent, screen, within } from '@testing-library/react';
import type { UserEvent } from '@testing-library/user-event';
import { useState, type ReactNode } from 'react';
import ChatPlace, { type ChatPlaceProps } from '../../../../src/places/chat/ChatPlace';
import { ToastProvider } from '../../../../src/contexts/ToastContext';
import { ConfirmProvider } from '../../../../src/contexts/ConfirmContext';
import { SettingsProvider } from '../../../../src/contexts/SettingsContext';
import { resetChats, useChats } from '../../../../src/services/chatRegistry';
import type { ChatMessage, ConversationSettings, ConversationSummary } from '../../../../src/services/transport';
import type { ConversationSettingsPatch } from '../../../../src/types/generated/ConversationSettingsPatch';
import type { DrawingAvailability } from '../../../../src/types/generated/DrawingAvailability';
import type { RunInfo } from '../../../../src/types/generated/RunInfo';

beforeEach(() => resetChats());

// jsdom has no ResizeObserver and assistant-ui's composer measures itself
// with one.
class NoopResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= NoopResizeObserver as unknown as typeof ResizeObserver;
// Nor does it scroll: the thread's viewport scrolls itself to the latest turn.
Element.prototype.scrollTo ??= function scrollTo() {};

export const wrapper = ({ children }: { children: ReactNode }) => (
  <ToastProvider>
    <ConfirmProvider>
      <SettingsProvider showToast={() => {}}>{children}</SettingsProvider>
    </ConfirmProvider>
  </ToastProvider>
);

export function conversation(id: number, title: string): ConversationSummary {
  return {
    id,
    title,
    model_id: null,
    system_prompt: null,
    created_at: '2026-09-01T09:00:00Z',
    updated_at: '2026-09-01T09:00:00Z',
  };
}

export function agentRun(id: string, conversationId: number, status: RunInfo['status']): RunInfo {
  return {
    id,
    kind: 'agent',
    status,
    created_at_ms: 1,
    conversation_id: conversationId,
    last_seq: 0,
    ...(status !== 'queued' && status !== 'in_progress' && { finished_at_ms: 2 }),
  };
}

/** A run's frames, then a stream held open until the reader leaves. */
export async function* framesThenWait(frames: object[], signal: AbortSignal) {
  let seq = 0;
  for (const frame of frames) {
    seq += 1;
    yield { type: 'frame' as const, seq, data: JSON.stringify(frame) };
  }
  await new Promise<void>((resolve) => signal.addEventListener('abort', () => resolve()));
}

/** A machine's answer when it has nothing to draw with: the Draw button is greyed. */
export const NO_DRAWING: DrawingAvailability = {
  available: false,
  code: 'drawing_unavailable',
  reason: 'no image model is installed; download one first',
  models: [],
};

export interface ChatFixture {
  conversations: ConversationSummary[];
  rows: Record<number, ChatMessage[]>;
  runs: RunInfo[];
  frames: Record<string, object[]>;
}

/** A transport over `fixture`, which a test may change as it goes. */
export function chatTransport(fixture: ChatFixture) {
  return {
    getServerToolSupport: vi.fn(async () => ({ supports_tool_calls: true, detected_format: null })),
    getModel: vi.fn(async () => ({ quantization: 'Q8_0' })),
    // The picker reads the endpoint's live dashboard while it is up, for which the key is revealed.
    revealSecret: vi.fn(async () => null),
    // A copy, as a fetch gives: the page must not see the fixture change under it.
    listConversations: vi.fn(async () => [...fixture.conversations]),
    createConversation: vi.fn(async (params: { title: string }) => {
      const id = Math.max(0, ...fixture.conversations.map((c) => c.id)) + 1;
      fixture.conversations.unshift(conversation(id, params.title));
      return id;
    }),
    // The patch made as the daemon makes it: a field sent replaces its setting.
    updateConversationSettings: vi.fn(async (id: number, patch: ConversationSettingsPatch) => {
      fixture.conversations = fixture.conversations.map((c) => {
        if (c.id !== id) return c;
        const settings: ConversationSettings = { ...c.settings };
        if (patch.tools !== undefined) settings.tools = patch.tools;
        if (patch.no_tools !== undefined) {
          if (patch.no_tools) settings.no_tools = true;
          else delete settings.no_tools;
        }
        return { ...c, settings };
      });
    }),
    deleteConversation: vi.fn(async (id: number) => {
      fixture.conversations = fixture.conversations.filter((c) => c.id !== id);
    }),
    getThread: vi.fn(async (id: number) => ({ messages: fixture.rows[id] ?? [] })),
    listRuns: vi.fn(async () => fixture.runs),
    readRunEvents: (id: string, _after: number, signal: AbortSignal) =>
      framesThenWait(fixture.frames[id] ?? [], signal),
    getSettings: vi.fn(async () => ({})),
    // Asked for the composer's Draw button; nothing to draw with unless a test says so.
    drawingAvailability: vi.fn(async () => NO_DRAWING),
    subscribe: vi.fn(() => () => {}),
    // Read by the composer's Tools ▸: no tool server unless a test says so.
    listMcpServers: vi.fn(async (): Promise<unknown[]> => []),
  };
}

/**
 * The place beside a stand-in for the sidebar's list: a listbox of the
 * store's chats, the open one selected, whose click opens another the way
 * the sidebar does, by handing the place a new `conversationId`. A chat the
 * place opens itself (a first send, a branch) is selected as the address
 * would follow it. The sidebar's own list is `tests/ts/sidebar/chatList.test.tsx`'s.
 */
export function ChatPlaceWithList(props: ChatPlaceProps) {
  const [open, setOpen] = useState(props.conversationId);
  const { list } = useChats(props.desk ? 'far' : 'this');
  return (
    <>
      <div role="listbox" aria-label="Conversations">
        {(list ?? []).map((c) => (
          <div key={c.id} role="option" aria-selected={c.id === open} tabIndex={0} onClick={() => setOpen(c.id)}>
            {c.title}
          </div>
        ))}
      </div>
      <ChatPlace
        {...props}
        conversationId={open}
        onOpened={(id) => {
          setOpen(id);
          props.onOpened?.(id);
        }}
      />
    </>
  );
}

/** Pick `item` from the chat's title ▾. */
export async function fromTitleMenu(user: UserEvent, item: string): Promise<void> {
  const heading = await screen.findByRole('heading', { level: 2 });
  await user.click(within(heading).getByRole('button'));
  await user.click(screen.getByRole('menuitem', { name: item }));
}

/** The composer's `[+]` button. */
export const plusButton = () => screen.getByRole('button', { name: 'Add to the message' });

/**
 * The `[+]` menu's item `name`, the menu opened for it if it is not open.
 * Synchronous, so a `waitFor` can ask it again as the composer remounts.
 */
export function plusItem(name: string | RegExp): HTMLElement {
  const plus = plusButton();
  if (plus.getAttribute('aria-expanded') !== 'true') fireEvent.click(plus);
  return screen.getByRole('menuitem', { name });
}

/** The submenu `name` of the `[+]` menu, opened. */
export async function plusSubmenu(user: UserEvent, name: 'Draw a picture' | 'Tools'): Promise<HTMLElement> {
  const item = plusItem(name);
  if (item.getAttribute('aria-expanded') !== 'true') await user.click(item);
  return screen.getByRole('menu', { name });
}

/** The Thinking toggle. */
export const thinkingSwitch = () => screen.getByRole('switch', { name: 'Thinking' });
/** The Thinking toggle, or null where none is drawn. */
export const noThinkingSwitch = () => screen.queryByRole('switch', { name: 'Thinking' });

/** The composer's model picker ▾. */
export const modelPicker = () => screen.getByRole('button', { name: /^Model: / });

/** The picker's choices in group `group`, by name. */
export async function pickerGroup(user: UserEvent, group: string): Promise<string[]> {
  const picker = modelPicker();
  if (picker.getAttribute('aria-expanded') !== 'true') await user.click(picker);
  const items = screen.getByRole('group', { name: group }).querySelectorAll('[role^="menuitem"]');
  return [...items].map((item) => item.textContent ?? '');
}

/** Pick `name` from the composer's model picker ▾. */
export async function pickModel(user: UserEvent, name: string): Promise<void> {
  const picker = modelPicker();
  if (picker.getAttribute('aria-expanded') !== 'true') await user.click(picker);
  await user.click(screen.getByRole('menuitemradio', { name }));
}
