/**
 * The inspector's far subject, at `/far/:id` through the real routes: the
 * machine and its reach in the remote status's own word, the summary its
 * row carries, Load and Chat only as that machine lists them and disabled
 * while it is away (saying why) or its rows could not be read, a failed
 * read as one line with Retry, Load reading the model again until
 * "● Loaded on desk", a Load's wait and failure kept to the model it was
 * pressed for, and Details with what that machine stores and no path.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

const far = vi.hoisted(() => ({
  actions: ['list', 'detail', 'chat', 'load'] as string[],
  failReads: 0,
  listFails: false,
  load: null as null | (() => Promise<unknown>),
}));
const transport = vi.hoisted(() => ({
  listModels: vi.fn(async () => []),
  listServers: vi.fn(async () => []),
  getDownloadQueue: vi.fn(async () => null),
  listPairedModels: vi.fn(),
  getPairedModel: vi.fn(),
  loadPairedModel: vi.fn(),
}));

vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return {
    ...actual,
    getTransport: () => ({
      ...transport,
      getSettings: vi.fn(async () => ({})),
      subscribe: vi.fn(() => () => {}),
      onEventStreamOpen: vi.fn(() => () => {}),
      revealSecret: vi.fn(async () => null),
    }),
  };
});
vi.mock('../../../src/services/transport/api/client', () => ({ get: vi.fn(async () => null) }));
vi.mock('../../../src/services/transport/api/setup', () => ({
  getSetupStatus: vi.fn(async () => ({ llamaInstalled: true })),
  getRecommendedModel: vi.fn(async () => null),
}));
vi.mock('../../../src/services/remoteEvents', () => ({ refreshRemoteStatus: vi.fn() }));
vi.mock('../../../src/pages/ChatPage', () => ({
  default: ({ modelName }: { modelName: string }) => <div data-testid="chat-page">{`Chatting with ${modelName}`}</div>,
}));

import { cleanupModelEvents, initModelEvents } from '../../../src/services/modelEvents';
import { IDLE_STATUS, applyRemoteStatus, resetRemoteState } from '../../../src/services/remoteRegistry';
import type { RemoteStatus } from '../../../src/services/transport/types/remote';
import type { ModelAction } from '../../../src/types/generated/ModelAction';
import { renderApp } from './harness';
import { farDetail, farEntry, pairedModels } from '../fixtures/fakeFarDaemon';

const DESK: RemoteStatus = {
  ...IDLE_STATUS,
  connected: { port: 41234, base_url: 'http://127.0.0.1:41234/v1', ticket_fingerprint: '3ca82708b995', path: 'direct', away_for_s: null },
  reach: 'connected',
  stored_ticket_fingerprint: '3ca82708b995',
  paired_name: 'desk',
};
const AWAY: RemoteStatus = { ...DESK, reach: 'away', connected: { ...DESK.connected!, away_for_s: 30 } };

/** The served state of each model on that machine, as its detail says. */
const serving = new Set<number>();

const inspector = () => screen.getByRole('complementary', { name: 'Inspector' });

function renderFar(id = 3) {
  applyRemoteStatus(DESK);
  initModelEvents();
  return renderApp(`/far/${id}`);
}

describe('the inspector, a model on the paired machine', () => {
  beforeEach(() => {
    far.actions = ['list', 'detail', 'chat', 'load'];
    far.failReads = 0;
    far.listFails = false;
    serving.clear();
    for (const mock of Object.values(transport)) mock.mockClear();
    transport.listPairedModels.mockImplementation(async () => {
      if (far.listFails) throw new Error('tunnel dropped');
      const group = pairedModels([farEntry('qwen3', 3, { quantization: 'Q4_K_M', capabilities: ['vision'] }), farEntry('llama', 4)]);
      return { ...group, actions: far.actions as ModelAction[] };
    });
    transport.getPairedModel.mockImplementation(async (id: number) => {
      if (far.failReads > 0) {
        far.failReads -= 1;
        throw new Error('no such model');
      }
      return { detail: farDetail(id, id === 3 ? 'qwen3' : 'llama', { hfRepoId: 'Qwen/Qwen3-8B-GGUF', isServing: serving.has(id), bitsPerWeight: 4.85, revision: { kind: 'sha', value: 'a'.repeat(40), short: 'aaaaaaa' } }) };
    });
    transport.loadPairedModel.mockImplementation(async (id: number) => {
      if (far.load) return far.load();
      serving.add(id);
      return { model: 'qwen3', started: true, context: 30000 };
    });
    resetRemoteState();
    try {
      localStorage.setItem('gglib.inspector.details', 'open');
    } catch {
      // no storage here
    }
  });
  afterEach(() => {
    cleanupModelEvents();
    far.load = null;
  });

  it("names the machine and its reach in the status's word, the row's summary, and Details without a path", async () => {
    renderFar();
    expect(await within(inspector()).findByRole('heading', { name: 'qwen3' })).toBeInTheDocument();
    expect(within(inspector()).getByText('on desk ·')).toBeInTheDocument();
    expect(within(inspector()).getByText('Connected')).toBeInTheDocument();
    expect(within(inspector()).getByText('Q4_K_M · sees images · qwen3 - 8B parameters, Q4_K_M')).toBeInTheDocument();

    expect(within(inspector()).getByText('Q4_K_M · 4.85 bpw')).toBeInTheDocument();
    expect(within(inspector()).getByText('aaaaaaa')).toHaveAttribute('title', 'a'.repeat(40));
    expect(within(inspector()).getByText('gglib chat 3 --remote')).toBeInTheDocument();
    expect(within(inspector()).queryByText('File')).not.toBeInTheDocument();

    act(() => applyRemoteStatus(AWAY));
    expect(within(inspector()).getByText('Away')).toBeInTheDocument();
  });

  it('offers only the actions that machine lists', async () => {
    far.actions = ['list', 'detail'];
    renderFar();
    await within(inspector()).findByRole('heading', { name: 'qwen3' });
    await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
    expect(within(inspector()).queryByRole('button', { name: /^(Chat|Load on desk)$/ })).not.toBeInTheDocument();
  });

  it('disables Load and Chat while the machine is away, and says why', async () => {
    renderFar();
    await within(inspector()).findByRole('button', { name: 'Load on desk' });
    act(() => applyRemoteStatus(AWAY));
    expect(within(inspector()).getByRole('button', { name: 'Load on desk' })).toBeDisabled();
    expect(within(inspector()).getByRole('button', { name: 'Chat' })).toBeDisabled();
    expect(within(inspector()).getByText('desk is away; Load and Chat wait until it is back.')).toBeInTheDocument();
  });

  it("disables them while that machine's rows could not be read, and Retry reads them again", async () => {
    renderFar();
    await within(inspector()).findByRole('button', { name: 'Load on desk' });
    far.listFails = true;
    act(() => window.dispatchEvent(new Event('focus')));
    const line = await within(inspector()).findByRole('alert');
    expect(line).toHaveTextContent("desk's models could not be read just now.");
    expect(within(inspector()).getByRole('button', { name: 'Load on desk' })).toBeDisabled();

    far.listFails = false;
    await userEvent.setup().click(within(line).getByRole('button', { name: 'Retry' }));
    await waitFor(() => expect(within(inspector()).getByRole('button', { name: 'Load on desk' })).toBeEnabled());
  });

  it('says a failed read in one line, and Retry reads it again', async () => {
    far.failReads = 1;
    renderFar();
    const line = await within(inspector()).findByRole('alert');
    expect(line).toHaveTextContent('It could not be read just now.');
    expect(within(inspector()).getByRole('button', { name: 'Chat' })).toBeDisabled();

    await userEvent.setup().click(within(line).getByRole('button', { name: 'Retry' }));
    expect(await within(inspector()).findByRole('heading', { name: 'qwen3' })).toBeInTheDocument();
    await waitFor(() => expect(within(inspector()).queryByRole('alert')).not.toBeInTheDocument());
    expect(within(inspector()).getByRole('button', { name: 'Chat' })).toBeEnabled();
  });

  it('Load loads it there, reads it again, then says Loaded on desk and offers Chat, not Load', async () => {
    renderFar();
    const user = userEvent.setup();
    await user.click(await within(inspector()).findByRole('button', { name: 'Load on desk' }));

    expect(await within(inspector()).findByText('Loaded on desk')).toBeInTheDocument();
    expect(transport.loadPairedModel).toHaveBeenCalledWith(3);
    expect(transport.getPairedModel).toHaveBeenCalledTimes(2);
    expect(within(inspector()).queryByRole('button', { name: 'Load on desk' })).not.toBeInTheDocument();
    await user.click(within(inspector()).getByRole('button', { name: 'Chat' }));
    expect(await screen.findByTestId('chat-page')).toHaveTextContent('Chatting with');
  });

  it("shows another model picked during a Load neither that Load's wait nor its failure", async () => {
    let fail: () => void = () => {};
    far.load = () => new Promise((_, reject) => (fail = () => reject(new Error('out of memory'))));
    const { router } = renderFar();
    await userEvent.setup().click(await within(inspector()).findByRole('button', { name: 'Load on desk' }));

    await act(() => router.navigate('/far/4'));
    await within(inspector()).findByRole('heading', { name: 'llama' });
    expect(within(inspector()).getByRole('button', { name: 'Load on desk' })).toBeEnabled();
    await act(async () => fail());
    expect(within(inspector()).queryByText(/Didn't load/)).not.toBeInTheDocument();
  });
});
