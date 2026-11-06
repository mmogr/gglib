/**
 * The chat routes: a local chat on a model nothing serves goes to its
 * inspector; a far chat's machine is fixed for its life.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, screen, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom';

import { farEntry, pairedModels } from '../fixtures/fakeFarDaemon';

const transport = vi.hoisted(() => ({
  subscribe: vi.fn(() => () => {}),
  listModels: vi.fn(async () => []),
  listPairedModels: vi.fn(async () => pairedModels([farEntry('qwen3-8b', 7)])),
  getSettings: vi.fn(async () => ({})),
}));
vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => transport,
}));
vi.mock('../../../src/pages/ModelControlCenterPage', () => ({ default: () => <main aria-label="Library" /> }));
// The chat page is a placard naming what it was given.
vi.mock('../../../src/pages/ChatPage', () => ({
  default: ({ modelName, serverPort, paired }: { modelName: string; serverPort?: number; paired?: { far: { id: number }; machineName: string } }) => (
    <div data-testid="chat-page" data-port={serverPort ?? ''} data-paired={paired ? `${paired.machineName}:${paired.far.id}` : ''}>
      Chatting with {modelName}
    </div>
  ),
}));

import { HANDED, peekChatHandoff, setChatHandoff } from '../../../src/app/chatHandoff';
import { IDLE_STATUS, applyRemoteStatus, ingestRemoteEvent, resetRemoteState } from '../../../src/services/remoteRegistry';
import { renderApp } from '../places/harness';

const DESK = '3ca82708b995';

function connectedTo(fingerprint: string, name: string) {
  return {
    ...IDLE_STATUS,
    connected: { port: 41234, base_url: 'http://127.0.0.1:41234/v1', ticket_fingerprint: fingerprint, path: 'direct' as const, away_for_s: null },
    paired_name: name,
  };
}

describe('the chat routes', () => {
  beforeEach(() => resetRemoteState());
  afterEach(() => resetRemoteState());

  it('a chat on a model nothing serves goes to its inspector', async () => {
    const { at } = renderApp('/chat/7');
    await waitFor(() => expect(at()).toBe('/models/7'));
    expect(screen.getByRole('main', { name: 'Library' })).toBeInTheDocument();
  });

  it('a handoff is read only by the navigation that set it, and taken once a page has committed', async () => {
    setChatHandoff({ modelId: 7, port: 5555, modelName: 'qwen3-8b' });
    // A plain visit: the address alone, not handed, so nothing serves 7; and
    // the page that committed drops what waited, which was not for it.
    const { at, router } = renderApp('/chat/7');
    await waitFor(() => expect(at()).toBe('/models/7'));
    expect(peekChatHandoff(7)).toBeNull();

    setChatHandoff({ modelId: 7, port: 5555, modelName: 'qwen3-8b' });
    await act(() => router.navigate('/chat/7', { state: HANDED }));

    const chat = await screen.findByTestId('chat-page');
    expect(chat).toHaveAttribute('data-port', '5555');
    expect(chat).toHaveTextContent('Chatting with qwen3-8b');
    expect(peekChatHandoff(7)).toBeNull();
  });

  it('a chat with no machine connected goes to the library', async () => {
    const { at } = renderApp('/chat/far/7');
    await waitFor(() => expect(at()).toBe('/library'));
  });

  it("opens on the paired machine's model, keeps across a drop and an unnamed connection, and closes once another machine answers", async () => {
    act(() => applyRemoteStatus(connectedTo(DESK, 'desk')));
    const { at } = renderApp('/chat/far/7');
    const chat = await screen.findByTestId('chat-page');
    expect(chat).toHaveTextContent('Chatting with qwen3-8b');
    expect(chat).toHaveAttribute('data-paired', 'desk:7');

    act(() => ingestRemoteEvent({ type: 'remote_disconnected' }));
    expect(screen.getByTestId('chat-page')).toBeInTheDocument();
    act(() => applyRemoteStatus(connectedTo(DESK, 'desk')));
    expect(screen.getByTestId('chat-page')).toBeInTheDocument();
    act(() => ingestRemoteEvent({ type: 'remote_joined', port: 41235 }));
    expect(screen.getByTestId('chat-page')).toBeInTheDocument();
    expect(at()).toBe('/chat/far/7');

    act(() => applyRemoteStatus(connectedTo('ffeeddccbbaa', 'study')));
    await waitFor(() => expect(at()).toBe('/library'));
  });
});
