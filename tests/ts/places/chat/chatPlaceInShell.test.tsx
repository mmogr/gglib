/**
 * A chat as the app draws it: the real routes, the sidebar and the place.
 * The sidebar's + opens an empty chat on the model the route picks, with no
 * dialog; its first send makes the conversation, the address becomes it
 * (replaced, so Back does not return to an empty chat) and the sidebar lists
 * and marks it; leaving it for the library unloads nothing (CH-10).
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { agentRun, chatTransport, type ChatFixture } from './chatPlaceHarness';

const transport = vi.hoisted(() => ({ current: {} as Record<string, unknown> }));
vi.mock('../../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../../src/services/transport');
  return { ...actual, getTransport: () => transport.current };
});
vi.mock('../../../../src/components/ProxyDashboardModal', () => ({ ProxyDashboardModal: () => null }));
vi.mock('../../../../src/places/library/LibraryPlace', () => ({ default: () => <main aria-label="Library" /> }));

import { resetChats } from '../../../../src/services/chatRegistry';
import { ingestServerEvent } from '../../../../src/services/serverRegistry';
import { cleanupServerEvents, forgetServerList, initServerEvents } from '../../../../src/services/serverEvents';
import { renderShell } from '../harness';

let fixture: ChatFixture;
// The whole window mounts here, assistant-ui and all: slow under a full run's load.
const SLOW = { timeout: 5000 };
const stopServer = vi.fn(async () => {});

beforeEach(() => {
  resetChats();
  stopServer.mockClear();
  fixture = { conversations: [], rows: {}, runs: [], frames: {} };
  transport.current = {
    ...chatTransport(fixture),
    listConversations: vi.fn(async () => [...fixture.conversations]),
    // The daemon saves the question as the run starts.
    startAgentRun: vi.fn(async (id: string, request: { conversation_id: number; messages: Array<{ content: unknown }> }) => {
      const cid = request.conversation_id;
      const content = String(request.messages.at(-1)?.content ?? '');
      fixture.rows[cid] = [...(fixture.rows[cid] ?? []), { id: 1, conversation_id: cid, role: 'user', content, created_at: '2026-11-07T09:00:00Z' }];
      return agentRun(id, cid, 'queued');
    }),
    stopServer,
    listModels: vi.fn(async () => []),
    getProxyStatus: vi.fn(async () => ({ running: false, state: 'off', port: null, base_url: null, key: null, pinned_model: null })),
    getRemoteStatus: vi.fn(async () => null),
    revealSecret: vi.fn(async () => null),
    // The list the app's server bridge reads before a chat decides what serves it.
    listServers: vi.fn(async () => [
      { model_id: 41, model_name: 'qwen3-8b', pid: 1, port: 5041, started_at: Math.floor(Date.now() / 1000), runtime: 'llama' },
    ]),
  };
  act(() => ingestServerEvent({ type: 'running', modelId: '41', port: 5041, updatedAt: Date.now(), modelName: 'qwen3-8b' }));
  forgetServerList();
  initServerEvents();
});
afterEach(() => {
  cleanupServerEvents();
  act(() => ingestServerEvent({ type: 'stopped', modelId: '41', port: 5041, updatedAt: Date.now() + 1 }));
});

describe('a chat in the window', () => {
  it("+ opens an empty chat at once; its first send's conversation becomes the address and the sidebar's marked row; leaving unloads nothing", { timeout: 20_000 }, async () => {
    const user = userEvent.setup();
    const app = renderShell('/library');
    const chats = screen.getByRole('region', { name: 'Chats' });
    expect(await within(chats).findByText('No chats yet.', {}, SLOW)).toBeInTheDocument();

    await user.click(within(chats).getByRole('button', { name: 'New chat' }));
    expect(app.at()).toBe('/chat/new');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    // The one loaded model, the only one there is.
    expect(await screen.findByText('qwen3-8b', {}, SLOW)).toBeInTheDocument();

    await user.type(await screen.findByRole('textbox', { name: 'Message' }, SLOW), 'Five largest files?{Enter}');

    await waitFor(() => expect(app.at()).toBe('/chat/1'), SLOW);
    expect(app.router.state.historyAction).toBe('REPLACE');
    const row = await within(chats).findByRole('button', { name: 'New Chat' }, SLOW);
    expect(row).toHaveAttribute('aria-current', 'page');
    expect(await screen.findByText('Five largest files?', {}, SLOW)).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /^Library/ }));
    expect(app.at()).toBe('/library');
    expect(stopServer).not.toHaveBeenCalled();
  });
});
