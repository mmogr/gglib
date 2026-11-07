/**
 * The chat page's folder, on this machine's model: chosen from `[+]` › Use
 * a folder…, then a chip by its name in the composer, drawn only while one
 * is set, × to clear it; a run says the folder only
 * when it changes it, the path chosen or `""` for Clear; and a run refused
 * for its folder hands the message back, says why above the composer, and
 * leaves the chip a warning until another folder is chosen or it is cleared.
 *
 * The daemon here remembers as the real one does: a run that says a folder
 * leaves its conversation remembering it, and one that says `""` without.
 * On web the path is typed.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import type { ConversationSettings } from '../../../../src/services/transport';
import type { AgentRunRequest } from '../../../../src/types/generated/AgentRunRequest';
import { TransportError } from '../../../../src/services/transport/errors';
import { FOLDER_GONE } from '../../../../src/hooks/useChatFolder';
import { agentRun, chatTransport, conversation, plusItem, wrapper, type ChatFixture } from './chatPlaceHarness';
import { guiModel } from '../../fixtures/model';

const transport = vi.hoisted(() => ({ current: {} as unknown }));
vi.mock('../../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../../src/services/transport');
  return { ...actual, getTransport: () => transport.current };
});

import ChatPlace from '../../../../src/places/chat/ChatPlace';

let fixture: ChatFixture;
/** The body of each run the page started, in order. */
let started: AgentRunRequest[];
/** Folders that are gone: a run in a chat remembering one is refused 409. */
let gone: Set<string>;

type Stub = ReturnType<typeof pageTransport>;
const stub = () => transport.current as Stub;

/** The settings of a chat after a run that said `folder`, as the daemon writes them. */
function remembering(settings: ConversationSettings | null | undefined, folder: string): ConversationSettings {
  const next: ConversationSettings = { ...settings };
  delete next.folder;
  if (folder !== '') next.folder = folder;
  return next;
}

function pageTransport() {
  let nextRow = 100;
  return {
    ...chatTransport(fixture),
    getModel: vi.fn(async () => guiModel({ id: 7, quantization: 'Q8_0' })),
    startAgentRun: vi.fn(async (id: string, request: AgentRunRequest) => {
      started.push(request);
      const cid = request.conversation_id as number;
      const settings = fixture.conversations.find((c) => c.id === cid)?.settings;
      const said = request.folder;
      if (said === undefined && settings?.folder && gone.has(settings.folder)) {
        throw new TransportError('CONFLICT', `no folder at '${settings.folder}'`, { status: 409, type: 'folder_missing' });
      }
      if (said !== undefined) {
        fixture.conversations = fixture.conversations.map((c) =>
          c.id === cid ? { ...c, settings: remembering(c.settings, said) } : c,
        );
      }
      const asked = String(request.messages.at(-1)?.content ?? '');
      fixture.rows[cid] = [
        ...(fixture.rows[cid] ?? []),
        { id: nextRow++, conversation_id: cid, role: 'user', content: asked, created_at: '2026-09-01T10:00:00Z' },
      ];
      return agentRun(id, cid, 'queued');
    }),
    readRunEvents: async function* (id: string, _after: number, _signal: AbortSignal) {
      yield { type: 'end' as const, info: agentRun(id, 1, 'completed') };
    },
  };
}

function renderLocal() {
  return render(<ChatPlace modelName="Qwen3.8-27B" modelId={7} serverPort={4321} conversationId={fixture.conversations[0]?.id ?? null} />, { wrapper });
}

const composer = () => screen.getByRole('textbox', { name: 'Message' });
const rowOf = (element: HTMLElement) => element.closest('form') as HTMLElement;
const margin = () => within(rowOf(composer()));

/** Send `text`, and wait for its run to start and end and for the list to be read after it. */
async function exchange(user: ReturnType<typeof userEvent.setup>, text: string): Promise<AgentRunRequest> {
  const runs = started.length;
  const reads = stub().listConversations.mock.calls.length;
  await user.type(composer(), `${text}{Enter}`);
  await waitFor(() => expect(started).toHaveLength(runs + 1));
  await waitFor(() => expect(stub().listConversations.mock.calls.length).toBeGreaterThan(reads));
  await waitFor(() => expect(screen.queryByRole('button', { name: 'Stop' })).not.toBeInTheDocument());
  return started[runs];
}

beforeEach(() => {
  window.localStorage.clear();
  started = [];
  gone = new Set();
  fixture = {
    conversations: [conversation(1, 'launchd KeepAlive'), conversation(2, 'Parsing GGUF')],
    rows: {
      1: [{ id: 11, conversation_id: 1, role: 'user', content: 'What does KeepAlive do?', created_at: '2026-09-01T09:12:00Z' }],
      2: [],
    },
    runs: [],
    frames: {},
  };
  transport.current = pageTransport();
});

describe('ChatPlace, the chat\'s folder', () => {
  it('says a folder typed here on the next run only, and shows it by name once the chat remembers it', async () => {
    const user = userEvent.setup();
    renderLocal();
    await screen.findByText('What does KeepAlive do?');

    expect((await exchange(user, 'first')).folder).toBeUndefined();

    // No chip until one is set.
    expect(margin().queryByRole('button', { name: 'Clear the folder' })).not.toBeInTheDocument();
    await user.click(plusItem('Use a folder…'));
    await user.type(screen.getByRole('textbox', { name: 'Folder path' }), '/home/m/src/gglib{Enter}');
    expect(margin().getByText('gglib')).toBeInTheDocument();
    expect(margin().getByText('gglib').closest('[title]')?.getAttribute('title')).toMatch(/^This chat's file tools read \/home\/m\/src\/gglib, read-only\./);

    expect((await exchange(user, 'read the README')).folder).toBe('/home/m/src/gglib');
    expect(fixture.conversations.find((c) => c.id === 1)?.settings?.folder).toBe('/home/m/src/gglib');
    expect((await exchange(user, 'and the CONTRIBUTING')).folder).toBeUndefined();
    expect(margin().getByText('gglib')).toBeInTheDocument();
  });

  it('takes a typed path on Enter without sending the message drafted beside it, and is no form inside the composer\'s', async () => {
    const user = userEvent.setup();
    const errors = vi.spyOn(console, 'error');
    renderLocal();
    await screen.findByText('What does KeepAlive do?');

    await user.type(composer(), 'half a thought');
    await user.click(plusItem('Use a folder…'));
    const field = screen.getByRole('textbox', { name: 'Folder path' });
    expect(field.closest('form')).toBe(rowOf(composer()));
    await user.type(field, '/home/m/src/gglib{Enter}');

    expect(margin().getByText('gglib')).toBeInTheDocument();
    expect(screen.queryByRole('textbox', { name: 'Folder path' })).not.toBeInTheDocument();
    expect(composer()).toHaveValue('half a thought');
    await act(async () => {});
    expect(started).toEqual([]);

    // Escape leaves it as it was, and sends nothing either.
    await user.click(margin().getByRole('button', { name: 'gglib' }));
    await user.type(screen.getByRole('textbox', { name: 'Folder path' }), '/elsewhere{Escape}');
    expect(margin().getByText('gglib')).toBeInTheDocument();
    expect(composer()).toHaveValue('half a thought');
    expect(started).toEqual([]);
    expect(errors.mock.calls.flat().join(' ')).not.toMatch(/cannot be a descendant of/);
    errors.mockRestore();
  });

  it('says "" on the run after Clear, and then nothing', async () => {
    const user = userEvent.setup();
    fixture.conversations[0].settings = { folder: '/home/m/src/gglib' };
    renderLocal();
    await screen.findByText('What does KeepAlive do?');
    expect(await margin().findByText('gglib')).toBeInTheDocument();

    await user.click(margin().getByRole('button', { name: 'Clear the folder' }));
    // Unset, the chip goes.
    expect(margin().queryByText('gglib')).not.toBeInTheDocument();
    expect(margin().queryByRole('button', { name: 'Clear the folder' })).not.toBeInTheDocument();
    expect((await exchange(user, 'no files now')).folder).toBe('');
    expect(fixture.conversations.find((c) => c.id === 1)?.settings?.folder).toBeUndefined();
    expect((await exchange(user, 'still none')).folder).toBeUndefined();
  });

  it('hands back a message refused for a remembered folder that is gone, says why, and Clear lets it through', async () => {
    const user = userEvent.setup();
    fixture.conversations[0].settings = { folder: '/home/m/gone' };
    gone.add('/home/m/gone');
    renderLocal();
    await screen.findByText('What does KeepAlive do?');
    await margin().findByText('gone');

    await user.type(composer(), 'read it{Enter}');
    await waitFor(() => expect(started).toHaveLength(1));
    expect(await screen.findByText(FOLDER_GONE)).toBeInTheDocument();
    await waitFor(() => expect(composer()).toHaveValue('read it'));
    expect(margin().getByText('gone').closest('[title]')).toHaveAttribute('title', `/home/m/gone: ${FOLDER_GONE}`);

    await user.click(margin().getByRole('button', { name: 'Clear the folder' }));
    await user.clear(composer());
    expect((await exchange(user, 'read it')).folder).toBe('');
  });
});
