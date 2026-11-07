/**
 * The endpoint place, driven through the real routes: the address line in
 * each state with the daemon's words (Turn on names no port, a failure is
 * the daemon's message as it said it, Copy); the loaded rows from the
 * dashboard (flag line, Reading %, the queue, the room for one more) and
 * each row's actions (Unload asks the daemon whatever this window
 * believes, pin and unpin restart the endpoint); Connect a client's
 * recipes for the model picked; Test it either way; Using it, Recent and
 * Activity; and the inspector beside the place, with a loaded model's rate
 * and time loaded. The words of a proof and of a recipe are pinned to the
 * CLI by `contracts/connectClient.test.ts`.
 *
 * The transport is a fake; the dashboard stream is played by the test.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

const library = vi.hoisted(() => ({ models: [] as unknown[] }));
const transport = vi.hoisted(() => ({
  listModels: vi.fn(async () => library.models),
  listServers: vi.fn(async () => [] as unknown[]),
  stopServer: vi.fn(async (_id: number) => {}),
  getDownloadQueue: vi.fn(async () => null),
  listPairedModels: vi.fn(async () => null),
  getModelDetail: vi.fn(async () => null),
  startProxy: vi.fn(async (_c?: unknown) => ({})),
  stopProxy: vi.fn(async () => {}),
  startPinnedProxy: vi.fn(async (_r: unknown) => ({})),
  proveProxy: vi.fn(async (_id: number | null): Promise<unknown> => ({})),
  getProxyClients: vi.fn(async (_id: number | null): Promise<unknown> => null),
  revealSecret: vi.fn(async (): Promise<string | null> => null),
  updateSettings: vi.fn(async (u: Record<string, unknown>) => u),
  // This machine's chats, for Chat on a model: its latest, else an empty one.
  listConversations: vi.fn(async (): Promise<unknown[]> => []),
}));
const settingsAnswer = vi.hoisted(() => ({ value: { proxyAutostart: false } as Record<string, unknown> }));
const dashboard = vi.hoisted(() => ({
  send: null as ((s: unknown) => void) | null,
  clear: vi.fn(async (_h: string, _p: number, _k?: string | null) => ({ status: 'ok', message: 'Prompt cache cleared' })),
}));

vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return {
    ...actual,
    getTransport: () => ({
      ...transport,
      getSettings: vi.fn(async () => settingsAnswer.value),
      subscribe: vi.fn(() => () => {}),
      onEventStreamOpen: vi.fn(() => () => {}),
    }),
  };
});
vi.mock('../../../src/services/transport/api/client', () => ({ get: vi.fn(async () => null), post: vi.fn(async () => null) }));
vi.mock('../../../src/services/transport/api/setup', () => ({
  getSetupStatus: vi.fn(async () => ({ llamaInstalled: true, llamaCanDownload: true })),
  getEngineStatus: vi.fn(async () => ({ installed: true })),
  streamEngineInstall: vi.fn(() => () => {}),
  getRecommendedModel: vi.fn(async () => null),
}));
vi.mock('../../../src/services/remoteEvents', () => ({ refreshRemoteStatus: vi.fn() }));
vi.mock('../../../src/services/platform/serverLogs', () => ({
  getServerLogs: vi.fn(async () => []),
  listenToServerLogs: vi.fn(async () => () => {}),
}));
vi.mock('../../../src/services/clients/proxyDashboard', () => ({
  subscribeProxyDashboard: (_h: string, _p: number, _k: unknown, onSnapshot: (s: unknown) => void) => {
    dashboard.send = onSnapshot;
    return () => {
      dashboard.send = null;
    };
  },
  clearProxyCache: (h: string, p: number, k?: string | null) => dashboard.clear(h, p, k),
}));
vi.mock('../../../src/components/ProxyDashboardModal', () => ({
  ProxyDashboardModal: ({ isOpen }: { isOpen: boolean }) => (isOpen ? <div role="dialog" aria-label="Live numbers" /> : null),
}));
vi.mock('../../../src/places/chat/ChatPlace', () => ({
  default: ({ serverPort, conversationId }: { serverPort?: number; conversationId: number | null }) => (
    <div data-testid="chat-page" data-port={serverPort} data-conversation={conversationId ?? 'none'} />
  ),
}));

import { ENDPOINT_WORDS, KEY_WORDS, RESIDENCY_WORDS } from '../../../src/components/status/words';
import { cleanupModelEvents, initModelEvents } from '../../../src/services/modelEvents';
import { applyProxyStatus, resetProxyState } from '../../../src/services/proxyRegistry';
import { IDLE_STATUS, applyRemoteStatus, resetRemoteState } from '../../../src/services/remoteRegistry';
import { ingestServerEvent } from '../../../src/services/serverRegistry';
import { resetChats } from '../../../src/services/chatRegistry';
import type { ProxyStatus } from '../../../src/services/transport/types/proxy';
import type { DashboardSnapshot } from '../../../src/services/transport/types/dashboard';
import type { RemoteDevice } from '../../../src/types/generated/RemoteDevice';
import { activeConnection, dashboardSnapshot, modelDefectCounts, recentRequest, residentSlot, samplingAudit } from '../fixtures/dashboard';
import { guiModel } from '../fixtures/model';
import { renderApp } from './harness';

const QWEN = guiModel({ id: 1, name: 'Qwen3-4B', openaiId: 'Qwen3-4B' });
const GEMMA = guiModel({ id: 2, name: 'Gemma-3-4B-It', openaiId: 'Gemma-3-4B-It' });

const UP: ProxyStatus = { running: true, state: 'on', port: 8080, base_url: 'http://127.0.0.1:8080/v1', key: 'none_needed', pinned_model: null };
const DOWN: ProxyStatus = { running: false, state: 'off', port: null, base_url: null, key: null, pinned_model: null };

function renderEndpoint(status: ProxyStatus = UP, path = '/endpoint') {
  library.models = [QWEN, GEMMA];
  act(() => applyProxyStatus(status));
  initModelEvents();
  return { ...renderApp(path), user: userEvent.setup() };
}

/** Play one dashboard frame, once the place has opened the stream. */
async function frame(overrides: Partial<DashboardSnapshot> = {}) {
  await waitFor(() => expect(dashboard.send).not.toBeNull());
  act(() => dashboard.send?.(dashboardSnapshot(overrides)));
}

/** The admission part of a frame, with these residents. */
const admitted = (slots: ReturnType<typeof residentSlot>[], extra: Partial<DashboardSnapshot['admission']> = {}) => ({
  admission: { ...dashboardSnapshot().admission, slots, ...extra },
});

const loadedRow = (name: string) => within(screen.getByRole('listbox', { name: 'Loaded models' })).getByRole('option', { name: new RegExp(name) });
const alert = () => screen.getByRole('alert');

describe('the endpoint place', () => {
  beforeEach(() => {
    resetChats();
    for (const mock of Object.values(transport)) mock.mockClear();
    transport.listConversations.mockImplementation(async () => []);
    transport.revealSecret.mockImplementation(async () => null);
    transport.getProxyClients.mockImplementation(async () => null);
    dashboard.clear.mockClear();
    settingsAnswer.value = { proxyAutostart: false };
    resetRemoteState();
  });
  afterEach(() => {
    act(() => resetProxyState());
    cleanupModelEvents();
  });

  describe('the address line', () => {
    it('off: says Off, what that means, and Turn on names nothing, not even a port', async () => {
      const { user } = renderEndpoint(DOWN);
      expect(screen.getByText(ENDPOINT_WORDS.off)).toBeInTheDocument();
      expect(screen.getByText("Apps can't reach your models while it's off. Loaded models stay in memory.")).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Turn off' })).not.toBeInTheDocument();

      await user.click(screen.getByRole('button', { name: 'Turn on' }));

      expect(transport.startProxy).toHaveBeenCalledTimes(1);
      expect(transport.startProxy).toHaveBeenCalledWith({});
    });

    it('off: Start when gglib launches is the stored setting', async () => {
      const { user } = renderEndpoint(DOWN);
      const box = await screen.findByRole('checkbox', { name: 'Start when gglib launches' });
      await waitFor(() => expect(box).toBeEnabled());
      expect(box).not.toBeChecked();
      await user.click(box);
      await waitFor(() => expect(transport.updateSettings).toHaveBeenCalledWith({ proxyAutostart: true }));
    });

    it('a failed save of Start when gglib launches says the daemon\'s message', async () => {
      transport.updateSettings.mockRejectedValueOnce(new Error('settings are read-only'));
      const { user } = renderEndpoint(DOWN);
      const box = await screen.findByRole('checkbox', { name: 'Start when gglib launches' });
      await waitFor(() => expect(box).toBeEnabled());
      await user.click(box);
      await waitFor(() => expect(alert()).toHaveTextContent(/^settings are read-only$/));
    });

    it('a failed start says the daemon\'s message as it said it', async () => {
      transport.startProxy.mockRejectedValueOnce(new Error('Port 8080 is used by python3 (pid 4411).'));
      const { user } = renderEndpoint(DOWN);
      await user.click(screen.getByRole('button', { name: 'Turn on' }));
      await waitFor(() => expect(alert()).toHaveTextContent(/^Port 8080 is used by python3 \(pid 4411\)\.$/));
    });

    it('on: says On, the address, what it means; Copy copies it; Turn off stops it', async () => {
      const { user } = renderEndpoint();
      expect(screen.getByText(ENDPOINT_WORDS.on)).toBeInTheDocument();
      expect(screen.getByText('Every model in your library answers here by name.')).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Turn on' })).not.toBeInTheDocument();

      await user.click(screen.getByRole('button', { name: 'Copy' }));
      expect(await navigator.clipboard.readText()).toBe('http://127.0.0.1:8080/v1');
      await waitFor(() => expect(screen.getByTestId('toasts')).toHaveTextContent('Endpoint address copied'));

      await user.click(screen.getByRole('button', { name: 'Turn off' }));
      expect(transport.stopProxy).toHaveBeenCalledTimes(1);
    });

    it('a failed stop says the daemon\'s message as it said it', async () => {
      transport.stopProxy.mockRejectedValueOnce(new Error('daemon gone'));
      const { user } = renderEndpoint();
      await user.click(screen.getByRole('button', { name: 'Turn off' }));
      await waitFor(() => expect(alert()).toHaveTextContent(/^daemon gone$/));
    });

    it('starting: says Starting, and offers neither Turn on nor Turn off', () => {
      renderEndpoint({ ...UP, state: 'starting' });
      expect(screen.getByText(ENDPOINT_WORDS.starting)).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: /^Turn (on|off)$/ })).not.toBeInTheDocument();
    });

    it('pinned: says Pinned and the one model it serves; Unpin stops, then starts every model', async () => {
      const { user } = renderEndpoint({ ...UP, state: 'pinned', pinned_model: 'Qwen3-4B' });
      expect(screen.getByText(ENDPOINT_WORDS.pinned)).toBeInTheDocument();
      expect(screen.getByText('only Qwen3-4B, other names are refused')).toBeInTheDocument();

      await user.click(screen.getByRole('button', { name: 'Unpin' }));

      await waitFor(() => expect(transport.startProxy).toHaveBeenCalledWith({}));
      expect(transport.stopProxy.mock.invocationCallOrder[0]).toBeLessThan(transport.startProxy.mock.invocationCallOrder[0]);
      expect(transport.startPinnedProxy).not.toHaveBeenCalled();
    });

    it('a key required is said, and shown only when asked', async () => {
      transport.revealSecret.mockImplementation(async () => 'k3y-5f3a');
      const { user } = renderEndpoint({ ...UP, key: 'required' });
      const line = screen.getByText(`Key ${KEY_WORDS.required.toLowerCase()}`).parentElement as HTMLElement;
      expect(within(line).queryByText('k3y-5f3a')).not.toBeInTheDocument();

      await user.click(within(line).getByRole('button', { name: 'Show' }));

      expect(await within(line).findByText('k3y-5f3a')).toBeInTheDocument();
    });
  });

  describe('Loaded', () => {
    it('draws each resident with its word and flag line, Reading % while a prompt is read, the queue, and the room', async () => {
      renderEndpoint();
      await frame({
        ...admitted(
          [
            residentSlot(1, 'Qwen3-4B', { state: 'working', launch: { ...residentSlot(1, 'x').launch!, state: 'working', flag_line: 'Q4_K_M · 32K · q8 KV · 2.5 GB' } }),
            residentSlot(2, 'Gemma-3-4B-It', { state: 'working', launch: { ...residentSlot(2, 'x').launch!, state: 'working' } }),
          ],
          {
            // 4.9 s: whole seconds, rounded down, as `gglib proxy dashboard` says it.
            queued: [{ model_name: 'Llama-3.2-3B', waiting: 2, oldest_wait_ms: 4_900 }],
            // The daemon's own sentence (`SecondarySlotStatus::available_with`).
            secondary_slot: { state: 'available', detail: 'Room for one more, about 9.0 GiB.', headroom_bytes: 9 * 1024 ** 3 },
          },
        ),
        active_connections: [
          activeConnection({ model_name: 'Gemma-3-4B-It', phase: 'processing_prompt', prompt_processed: 61, prompt_total: 100 }),
          // Generating keeps its prompt's totals (`mark_generating` moves only the phase): not a reading.
          activeConnection({ id: 'conn-2', model_name: 'Qwen3-4B', phase: 'generating', prompt_processed: 100, prompt_total: 100 }),
        ],
      });

      expect(loadedRow('Qwen3-4B')).toHaveTextContent('Q4_K_M · 32K · q8 KV · 2.5 GB');
      expect(within(loadedRow('Qwen3-4B')).getByText(RESIDENCY_WORDS.working)).toBeInTheDocument();
      expect(within(loadedRow('Gemma-3-4B-It')).getByText(`${RESIDENCY_WORDS.reading} 61%`)).toBeInTheDocument();
      expect(within(loadedRow('Gemma-3-4B-It')).getByRole('progressbar')).toHaveAttribute('aria-valuenow', '61');
      expect(within(loadedRow('Qwen3-4B')).queryByRole('progressbar')).not.toBeInTheDocument();
      expect(screen.getByText('Llama-3.2-3B waiting for a slot · 2 requests · 4s')).toBeInTheDocument();
      expect(screen.getByText('Room for one more, about 9.0 GiB.')).toBeInTheDocument();
    });

    it('says no room when the second slot has no headroom to say', async () => {
      renderEndpoint();
      await frame(admitted([residentSlot(1, 'Qwen3-4B')], { secondary_slot: { state: 'no_headroom', detail: 'Room for one more, but not now.' } }));
      expect(loadedRow('Qwen3-4B')).toBeInTheDocument();
      expect(screen.queryByText(/^Room for one more/)).not.toBeInTheDocument();
    });

    it('with nothing loaded, says how a model gets loaded; Load a model… opens the palette', async () => {
      const { user, at } = renderEndpoint();
      await frame();
      expect(screen.getByText('Nothing loaded. An app loads a model by asking for it by name, or load one now.')).toBeInTheDocument();
      await user.click(screen.getByRole('button', { name: 'Load a model…' }));
      expect(screen.getByRole('dialog', { name: 'Load a model' })).toBeInTheDocument();
      expect(at()).toBe('/endpoint');
    });

    it('Unload asks the daemon even when this window has not heard the model is loaded', async () => {
      const { user } = renderEndpoint();
      await frame(admitted([residentSlot(1, 'Qwen3-4B')]));
      await user.click(within(loadedRow('Qwen3-4B')).getByRole('button', { name: 'More for Qwen3-4B' }));
      await user.click(screen.getByRole('menuitem', { name: 'Unload' }));
      expect(transport.stopServer).toHaveBeenCalledWith(1);

      await user.click(within(loadedRow('Qwen3-4B')).getByRole('button', { name: 'Unload' }));
      expect(transport.stopServer).toHaveBeenCalledTimes(2);
    });

    it('Pin the endpoint to it stops the endpoint, then starts it pinned to that model', async () => {
      const { user } = renderEndpoint();
      await frame(admitted([residentSlot(1, 'Qwen3-4B')]));
      await user.click(within(loadedRow('Qwen3-4B')).getByRole('button', { name: 'More for Qwen3-4B' }));
      await user.click(screen.getByRole('menuitem', { name: 'Pin the endpoint to it' }));

      await waitFor(() => expect(transport.startPinnedProxy).toHaveBeenCalledWith({ model_id: 1 }));
      expect(transport.stopProxy.mock.invocationCallOrder[0]).toBeLessThan(transport.startPinnedProxy.mock.invocationCallOrder[0]);
    });

    it('Chat with it, Show log and Change launch options… go where they say', async () => {
      const { user, at } = renderEndpoint();
      await frame(admitted([residentSlot(1, 'Qwen3-4B')]));
      const pick = async (item: string) => {
        await user.click(within(loadedRow('Qwen3-4B')).getByRole('button', { name: 'More for Qwen3-4B' }));
        await user.click(screen.getByRole('menuitem', { name: item }));
      };

      await pick('Change launch options…');
      // The sheet opens beside the place; only `serve` leaves the address.
      expect(await screen.findByRole('heading', { name: 'Load with options' })).toBeInTheDocument();
      expect(at()).toBe('/endpoint?inspect=model%3A1');

      await pick('Show log');
      expect(at()).toBe('/models/1/log');
    });

    it('Chat with it opens a chat on the model, handed the slot\'s port before the server list has heard of it', async () => {
      const { user, at } = renderEndpoint();
      await frame(admitted([residentSlot(1, 'Qwen3-4B', { port: 9431 })]));
      await user.click(within(loadedRow('Qwen3-4B')).getByRole('button', { name: 'More for Qwen3-4B' }));
      await user.click(screen.getByRole('menuitem', { name: 'Chat with it' }));
      // No chat on it yet: an empty one on it.
      await waitFor(() => expect(at()).toBe('/chat/new?model=1'));
      expect(await screen.findByTestId('chat-page')).toHaveAttribute('data-port', '9431');
    });

    it("a row's Chat opens the model's latest chat, handed the slot's port", async () => {
      transport.listConversations.mockImplementation(async () => [
        { id: 4, title: 'older', model_id: 1, system_prompt: null, created_at: '2026-11-01 09:00:00', updated_at: '2026-11-01 09:00:00' },
        { id: 5, title: 'newer', model_id: 1, system_prompt: null, created_at: '2026-11-02 09:00:00', updated_at: '2026-11-02 09:00:00' },
        { id: 6, title: 'other model', model_id: 2, system_prompt: null, created_at: '2026-11-03 09:00:00', updated_at: '2026-11-03 09:00:00' },
      ]);
      const { user, at } = renderEndpoint();
      await frame(admitted([residentSlot(1, 'Qwen3-4B', { port: 9431 })]));
      await user.click(within(loadedRow('Qwen3-4B')).getByRole('button', { name: 'Chat' }));
      await waitFor(() => expect(at()).toBe('/chat/5'));
      const chat = await screen.findByTestId('chat-page');
      expect(chat).toHaveAttribute('data-port', '9431');
      expect(chat).toHaveAttribute('data-conversation', '5');
    });

    it('Pin waits while a pin change is on its way', async () => {
      let stopped = () => {};
      transport.stopProxy.mockImplementationOnce(() => new Promise<void>((resolve) => (stopped = resolve)));
      const { user } = renderEndpoint();
      await frame(admitted([residentSlot(1, 'Qwen3-4B')]));
      const more = () => within(loadedRow('Qwen3-4B')).getByRole('button', { name: 'More for Qwen3-4B' });
      await user.click(more());
      await user.click(screen.getByRole('menuitem', { name: 'Pin the endpoint to it' }));
      await user.click(more());
      expect(screen.getByRole('menuitem', { name: 'Pin the endpoint to it' })).toBeDisabled();
      await act(async () => stopped());
    });
  });

  describe('Serves', () => {
    it('choosing one model restarts the endpoint pinned to it; Every model restarts it plain', async () => {
      const { user } = renderEndpoint();
      await frame(admitted([residentSlot(2, 'Gemma-3-4B-It')]));
      const serves = screen.getByRole('combobox', { name: 'Serves' });
      expect(within(serves).getByRole('option', { name: 'Only Gemma-3-4B-It' })).toBeInTheDocument();

      await user.selectOptions(serves, 'Only Qwen3-4B');
      await waitFor(() => expect(transport.startPinnedProxy).toHaveBeenCalledWith({ model_id: 1 }));
      expect(transport.stopProxy).toHaveBeenCalledTimes(1);
    });

    it('a pinned endpoint shows its model chosen, and Every model unpins it', async () => {
      const { user } = renderEndpoint({ ...UP, state: 'pinned', pinned_model: 'Qwen3-4B' });
      await screen.findByRole('option', { name: 'Only Qwen3-4B' });
      const serves = screen.getByRole('combobox', { name: 'Serves' });
      await waitFor(() => expect(serves).toHaveValue('1'));

      await user.selectOptions(serves, 'Every model');
      await waitFor(() => expect(transport.startProxy).toHaveBeenCalledWith({}));
      expect(transport.stopProxy.mock.invocationCallOrder[0]).toBeLessThan(transport.startProxy.mock.invocationCallOrder[0]);
    });
  });

  describe('pin and unpin restart the endpoint', () => {
    it('starts pinned only once the stop has landed', async () => {
      let stopped = () => {};
      transport.stopProxy.mockImplementationOnce(() => new Promise<void>((resolve) => (stopped = resolve)));
      const { user } = renderEndpoint();
      await frame();
      await user.selectOptions(screen.getByRole('combobox', { name: 'Serves' }), 'Only Qwen3-4B');
      expect(transport.stopProxy).toHaveBeenCalledTimes(1);
      expect(transport.startPinnedProxy).not.toHaveBeenCalled();

      await act(async () => stopped());
      await waitFor(() => expect(transport.startPinnedProxy).toHaveBeenCalledWith({ model_id: 1 }));
    });

    it('a pinned start that fails after the stop says the daemon\'s message as it said it', async () => {
      transport.startPinnedProxy.mockRejectedValueOnce(new Error('Qwen3-4B is not a chat model'));
      const { user } = renderEndpoint();
      await frame();
      await user.selectOptions(screen.getByRole('combobox', { name: 'Serves' }), 'Only Qwen3-4B');
      await waitFor(() => expect(alert()).toHaveTextContent(/^Qwen3-4B is not a chat model$/));
    });

    it('an unpin whose start fails after the stop says the daemon\'s message as it said it', async () => {
      transport.startProxy.mockRejectedValueOnce(new Error('port 8080 is in use'));
      const { user } = renderEndpoint({ ...UP, state: 'pinned', pinned_model: 'Qwen3-4B' });
      await user.click(screen.getByRole('button', { name: 'Unpin' }));
      await waitFor(() => expect(alert()).toHaveTextContent(/^port 8080 is in use$/));
      expect(transport.stopProxy).toHaveBeenCalledTimes(1);
    });
  });

  describe('Connect a client', () => {
    const answer = (modelId: string, gglibId: number, key: 'required' | 'none_needed' = 'none_needed') => ({
      base_url: UP.base_url,
      key,
      model_id: modelId,
      gglib_id: gglibId,
      recipes: [
        { client: 'Open WebUI', hint: 'Settings -> Connections -> OpenAI API', lines: [{ label: 'URL', value: UP.base_url }, { label: 'Key', value: 'gglib' }] },
      ],
    });

    it('reads the recipes for the last loaded model, and again for the model picked', async () => {
      transport.getProxyClients.mockImplementation(async (id) => (id === 1 ? answer('Qwen3-4B', 1) : answer('Gemma-3-4B-It', 2)));
      const { user } = renderEndpoint();
      await frame(admitted([residentSlot(1, 'Qwen3-4B', { resident_for_secs: 900 }), residentSlot(2, 'Gemma-3-4B-It', { resident_for_secs: 30 })]));
      const connect = screen.getByRole('region', { name: 'Connect a client' });

      await waitFor(() => expect(transport.getProxyClients).toHaveBeenLastCalledWith(2));
      expect(await within(connect).findByText('Gemma-3-4B-It', { selector: 'code' })).toBeInTheDocument();
      expect(within(connect).getByText(`${KEY_WORDS.none_needed.toLowerCase()} · type any text`)).toBeInTheDocument();

      await user.selectOptions(within(connect).getByRole('combobox', { name: 'Model to connect with' }), 'Qwen3-4B');
      await waitFor(() => expect(transport.getProxyClients).toHaveBeenLastCalledWith(1));
      expect(await within(connect).findByText('Qwen3-4B', { selector: 'code' })).toBeInTheDocument();
      const webui = within(connect).getByRole('group', { name: 'Open WebUI' });
      expect(within(webui).getByText('gglib')).toBeInTheDocument();
    });

    it('names the pinned model before the last loaded one', async () => {
      renderEndpoint({ ...UP, state: 'pinned', pinned_model: 'Gemma-3-4B-It' });
      await frame(admitted([residentSlot(1, 'Qwen3-4B')]));
      await waitFor(() => expect(transport.getProxyClients).toHaveBeenLastCalledWith(2));
    });

    it('with nothing loaded, names the default model, else the smallest, never the first', async () => {
      const big = guiModel({ id: 3, name: 'Qwen3.8-27B', sizeBytes: 29 * 1024 ** 3 });
      const small = guiModel({ id: 4, name: 'Qwen3-0.6B', sizeBytes: 1024 ** 3 });
      const mid = guiModel({ id: 5, name: 'Gemma-3-4B', sizeBytes: 3 * 1024 ** 3 });
      const open = async (defaultModelId: number | null) => {
        settingsAnswer.value = { proxyAutostart: false, defaultModelId };
        transport.getProxyClients.mockClear();
        library.models = [big, small, mid];
        act(() => applyProxyStatus(UP));
        initModelEvents();
        const view = renderApp('/endpoint');
        await frame();
        return view;
      };
      const first = await open(5);
      await waitFor(() => expect(transport.getProxyClients).toHaveBeenLastCalledWith(5));
      first.unmount();
      cleanupModelEvents();

      await open(null);
      await waitFor(() => expect(transport.getProxyClients).toHaveBeenLastCalledWith(4));
    });

    it('drops an answer for a model no longer picked', async () => {
      let late: (v: unknown) => void = () => {};
      transport.getProxyClients.mockImplementation((id) =>
        id === 2 ? new Promise((resolve) => (late = resolve)) : Promise.resolve(answer('Qwen3-4B', 1)),
      );
      const { user } = renderEndpoint(UP, '/endpoint?inspect=model:2');
      const connect = screen.getByRole('region', { name: 'Connect a client' });
      await waitFor(() => expect(transport.getProxyClients).toHaveBeenLastCalledWith(2));
      await within(connect).findByRole('option', { name: 'Qwen3-4B' });
      await user.selectOptions(within(connect).getByRole('combobox', { name: 'Model to connect with' }), 'Qwen3-4B');
      expect(await within(connect).findByText('Qwen3-4B', { selector: 'code' })).toBeInTheDocument();

      await act(async () => late(answer('Gemma-3-4B-It', 2)));
      expect(within(connect).getByText('Qwen3-4B', { selector: 'code' })).toBeInTheDocument();
      expect(within(connect).queryByText('Gemma-3-4B-It', { selector: 'code' })).not.toBeInTheDocument();
    });

    it("drops the last model's recipes as soon as another is picked", async () => {
      transport.getProxyClients.mockImplementation((id) => (id === 2 ? Promise.resolve(answer('Gemma-3-4B-It', 2)) : new Promise(() => {})));
      const { user } = renderEndpoint(UP, '/endpoint?inspect=model:2');
      const connect = screen.getByRole('region', { name: 'Connect a client' });
      expect(await within(connect).findByText('Gemma-3-4B-It', { selector: 'code' })).toBeInTheDocument();
      await user.selectOptions(within(connect).getByRole('combobox', { name: 'Model to connect with' }), 'Qwen3-4B');
      // Qwen's answer has not come: its Model ID is the library's, and Gemma's recipes are gone.
      expect(within(connect).getByText('Qwen3-4B', { selector: 'code' })).toBeInTheDocument();
      expect(within(connect).queryByText('Gemma-3-4B-It', { selector: 'code' })).not.toBeInTheDocument();
      expect(within(connect).queryByRole('group', { name: 'Open WebUI' })).not.toBeInTheDocument();
    });

    it("says why the recipes could not be read, in the daemon's words", async () => {
      transport.getProxyClients.mockRejectedValue(new Error('the endpoint is off'));
      renderEndpoint(UP, '/endpoint?inspect=model:1');
      const connect = screen.getByRole('region', { name: 'Connect a client' });
      expect(await within(connect).findByText('the endpoint is off')).toBeInTheDocument();
    });

    it('with a key required, a key line names the stored secret, never a key', async () => {
      transport.getProxyClients.mockImplementation(async () => answer('Qwen3-4B', 1, 'required'));
      renderEndpoint({ ...UP, key: 'required' }, '/endpoint?inspect=model:1');
      const webui = await screen.findByRole('group', { name: 'Open WebUI' });
      expect(transport.getProxyClients).toHaveBeenLastCalledWith(1);
      // Two spaces, as the CLI prints it: matched without collapsing them.
      expect(within(webui).getByText('(set, not shown)  `gglib config settings reveal proxy-api-key` prints it', { normalizer: (t) => t })).toBeInTheDocument();
      expect(within(webui).queryByText('gglib')).not.toBeInTheDocument();
    });
  });

  describe('Test it', () => {
    it('asks for the model Connect a client names, and says who answered and how fast', async () => {
      transport.proveProxy.mockImplementation(async () => ({ answered_by: 'Qwen3-4B', asked_for: 'Qwen3-4B', elapsed_ms: 1_350 }));
      const { user } = renderEndpoint(UP, '/endpoint?inspect=model:1');
      await user.click(screen.getByRole('button', { name: 'Test it' }));
      expect(transport.proveProxy).toHaveBeenCalledWith(1);
      expect(await screen.findByRole('status')).toHaveTextContent('answered in 1.4s by Qwen3-4B');
    });

    it('says Testing… at once, while the proof waits for the model to load', async () => {
      transport.proveProxy.mockImplementationOnce(() => new Promise(() => {}));
      const { user } = renderEndpoint(UP, '/endpoint?inspect=model:1');
      await user.click(screen.getByRole('button', { name: 'Test it' }));
      expect(screen.getByRole('status')).toHaveTextContent(/^Testing…$/);
    });

    it('says why the first request did not complete, in the daemon\'s words', async () => {
      transport.proveProxy.mockRejectedValueOnce(new Error('Qwen3-4B did not load: out of memory'));
      const { user } = renderEndpoint(UP, '/endpoint?inspect=model:1');
      await user.click(screen.getByRole('button', { name: 'Test it' }));
      expect(await screen.findByRole('status')).toHaveTextContent('the first request did not complete: Qwen3-4B did not load: out of memory');
    });

    it('a pinned endpoint is proved with its pin', async () => {
      const { user } = renderEndpoint({ ...UP, state: 'pinned', pinned_model: 'Qwen3-4B' }, '/endpoint?inspect=model:2');
      await user.click(screen.getByRole('button', { name: 'Test it' }));
      expect(transport.proveProxy).toHaveBeenCalledWith(null);
    });
  });

  describe('Using it, Recent and Activity', () => {
    const device = (over: Partial<RemoteDevice>): RemoteDevice => ({
      id: 'dev-1', joined_at: 0, redeemed_at: null, last_seen: null, peer: null, admitted: null, recorded: true, description: '', joined: true, ...over,
    });

    it('says how many requests are in flight and one line per joined device in the daemon\'s words', async () => {
      act(() =>
        applyRemoteStatus({
          ...IDLE_STATUS,
          devices: [device({ id: 'a', label: 'Laptop', description: 'joined 2 Nov · seen 3 min ago' }), device({ id: 'b', label: 'Phone', joined: false, description: 'invited' })],
        }),
      );
      renderEndpoint();
      await frame({ active_connections: [activeConnection(), activeConnection({ id: 'conn-2' })], total_requests: 9 });
      const using = screen.getByRole('region', { name: 'Using it' });
      expect(within(using).getByText('Apps on this Mac · 2 requests now')).toBeInTheDocument();
      expect(within(using).getByText('Laptop · joined 2 Nov · seen 3 min ago')).toBeInTheDocument();
      expect(within(using).queryByText(/Phone/)).not.toBeInTheDocument();
    });

    it("the menu's Connect a Client… opens the place at the block, and the ask leaves the address", async () => {
      const scrolled: Element[] = [];
      const before = Element.prototype.scrollIntoView;
      Element.prototype.scrollIntoView = function (this: Element) {
        scrolled.push(this);
      };
      try {
        const { at } = renderEndpoint(UP, '/endpoint?connect=1');
        const connect = await screen.findByRole('region', { name: 'Connect a client' });
        await waitFor(() => expect(at()).toBe('/endpoint'));
        expect(scrolled.filter((el) => el.contains(connect))).toHaveLength(1);
      } finally {
        Element.prototype.scrollIntoView = before;
      }
    });

    it('with the endpoint off, the ask to connect leaves the address at once and scrolls nothing later', async () => {
      const scrolled: Element[] = [];
      const before = Element.prototype.scrollIntoView;
      Element.prototype.scrollIntoView = function (this: Element) {
        scrolled.push(this);
      };
      try {
        const { at } = renderEndpoint(DOWN, '/endpoint?connect=1');
        await waitFor(() => expect(at()).toBe('/endpoint'));
        act(() => applyProxyStatus(UP));
        expect(await screen.findByRole('region', { name: 'Connect a client' })).toBeInTheDocument();
        expect(scrolled).toHaveLength(0);
      } finally {
        Element.prototype.scrollIntoView = before;
      }
    });

    it('an ask to connect that comes before the first status read waits for it, then scrolls', async () => {
      const scrolled: Element[] = [];
      const before = Element.prototype.scrollIntoView;
      Element.prototype.scrollIntoView = function (this: Element) {
        scrolled.push(this);
      };
      try {
        library.models = [QWEN, GEMMA];
        act(() => resetProxyState());
        initModelEvents();
        const { at } = renderApp('/endpoint?connect=1');
        await act(async () => {});
        expect(at()).toBe('/endpoint?connect=1');
        act(() => applyProxyStatus(UP));
        const connect = await screen.findByRole('region', { name: 'Connect a client' });
        await waitFor(() => expect(at()).toBe('/endpoint'));
        expect(scrolled.filter((el) => el.contains(connect))).toHaveLength(1);
      } finally {
        Element.prototype.scrollIntoView = before;
      }
    });

    it('a pinned endpoint takes the ask to connect as an endpoint that is on', async () => {
      const scrolled: Element[] = [];
      const before = Element.prototype.scrollIntoView;
      Element.prototype.scrollIntoView = function (this: Element) {
        scrolled.push(this);
      };
      try {
        const { at } = renderEndpoint({ ...UP, state: 'pinned', pinned_model: 'Qwen3-4B' }, '/endpoint?connect=1');
        const connect = await screen.findByRole('region', { name: 'Connect a client' });
        await waitFor(() => expect(at()).toBe('/endpoint'));
        expect(scrolled.filter((el) => el.contains(connect))).toHaveLength(1);
      } finally {
        Element.prototype.scrollIntoView = before;
      }
    });

    it('before anything has used it, says so and offers Connect a client', async () => {
      renderEndpoint();
      await frame();
      const using = screen.getByRole('region', { name: 'Using it' });
      expect(within(using).getByText('No app has used it yet.')).toBeInTheDocument();
      expect(within(using).getByRole('button', { name: 'Connect a client' })).toBeInTheDocument();
    });

    it('Recent is the last five, newest first, with tokens, rate and what gglib did; All › opens the live numbers', async () => {
      const { user } = renderEndpoint();
      const at = (n: number) => 1_800_000_000 + n * 60;
      await frame({
        recent_requests: [
          ...[0, 1, 2, 3].map((n) => recentRequest({ model_name: `m${n}`, recorded_at_secs: at(n) })),
          recentRequest({ model_name: 'm4', recorded_at_secs: at(4), completion_tokens: 1, generation_ms: 0 }),
          recentRequest({ model_name: 'm5', recorded_at_secs: at(5) }),
          recentRequest({
            model_name: 'Qwen3-4B',
            recorded_at_secs: at(6),
            completion_tokens: 812,
            generation_ms: 24_000,
            interventions: [
              { what: { kind: 'repaired_tool_call' }, word: 'Repaired a tool call' },
              { what: { kind: 'trimmed', messages: 4 }, word: 'Trimmed 4 old messages' },
            ],
          }),
        ],
      });
      const rows = within(screen.getByRole('list', { name: 'Recent requests' })).getAllByRole('listitem');
      expect(rows.map((r) => r.children[1].textContent)).toEqual(['Qwen3-4B', 'm5', 'm4', 'm3', 'm2']);
      expect(rows[0]).toHaveTextContent('812 tokens · 34 t/s');
      expect(rows[0]).toHaveTextContent('Repaired a tool call · Trimmed 4 old messages');
      expect(rows[1].textContent).not.toMatch(/token|t\/s/);
      // One token, written in no time: singular, and no rate.
      expect(rows[2]).toHaveTextContent(/1 token(?!s)/);
      expect(rows[2].textContent).not.toMatch(/t\/s/);

      await user.click(screen.getByRole('button', { name: 'All ›' }));
      expect(screen.getByRole('dialog', { name: 'Live numbers' })).toBeInTheDocument();
    });

    it('Recent is not drawn before the first request', async () => {
      renderEndpoint();
      await frame();
      expect(screen.getByRole('region', { name: 'Using it' })).toBeInTheDocument();
      expect(screen.queryByRole('region', { name: 'Recent' })).not.toBeInTheDocument();
    });

    it('Activity says the rest in sentences, and Clear presents the revealed key', async () => {
      transport.revealSecret.mockImplementation(async () => 'k3y');
      const { user } = renderEndpoint({ ...UP, key: 'required' });
      await frame({
        tool_repairs_succeeded: 3,
        per_model_defects: { 'Qwen3-4B': modelDefectCounts({ loop_guard_trips: 1 }) },
        sampling_audit: samplingAudit({ client_fields_discarded: 12 }),
        cache: {
          disk_enabled: false,
          disk_suppressed_for_model: false,
          ram_budget_mb: null,
          ram_state: 'healthy',
          needs_attention: false,
          warnings: [],
          usage: { reporting_requests: 4, unreported_requests: 0, prompt_tokens: 1000, cached_tokens: 710, last_prompt_tokens: null, last_cached_tokens: null },
        },
      });
      const activity = screen.getByRole('group', { name: 'Activity' });
      expect(activity).toHaveTextContent('Prompt cache 71% reused · 3 tool calls repaired · 1 loop-guard trip · 12 client sampling fields ignored');

      await user.click(within(activity).getByRole('button', { name: 'Clear prompt cache' }));
      expect(dashboard.clear).toHaveBeenCalledWith('127.0.0.1', 8080, 'k3y');
      await waitFor(() => expect(screen.getByTestId('toasts')).toHaveTextContent('Prompt cache cleared'));
    });

    it('Clear waits for the reveal: sent before it answers, no key would go and a keyed proxy refuses', async () => {
      transport.revealSecret.mockImplementation(() => new Promise(() => {}));
      const { user } = renderEndpoint({ ...UP, key: 'required' });
      await waitFor(() => expect(transport.revealSecret).toHaveBeenCalled());
      // The dashboard waits for the key too, so Activity is not drawn; ⋯ is.
      expect(screen.queryByRole('group', { name: 'Activity' })).not.toBeInTheDocument();
      await user.click(screen.getByRole('button', { name: 'More' }));
      expect(screen.getByRole('menuitem', { name: 'Clear prompt cache' })).toBeDisabled();
      expect(dashboard.clear).not.toHaveBeenCalled();
    });

    it('while off, ⋯ offers neither Live numbers nor Clear, and a dialog never opens by itself later', async () => {
      const { user } = renderEndpoint();
      await frame();
      await user.click(screen.getByRole('button', { name: 'More' }));
      await user.click(screen.getByRole('menuitem', { name: 'Live numbers' }));
      expect(screen.getByRole('dialog', { name: 'Live numbers' })).toBeInTheDocument();

      act(() => applyProxyStatus(DOWN));
      expect(screen.queryByRole('dialog', { name: 'Live numbers' })).not.toBeInTheDocument();
      await user.click(screen.getByRole('button', { name: 'More' }));
      expect(screen.getByRole('menuitem', { name: 'Live numbers' })).toBeDisabled();
      expect(screen.getByRole('menuitem', { name: 'Clear prompt cache' })).toBeDisabled();

      act(() => applyProxyStatus(UP));
      await waitFor(() => expect(screen.getByRole('button', { name: 'Turn off' })).toBeInTheDocument());
      expect(screen.queryByRole('dialog', { name: 'Live numbers' })).not.toBeInTheDocument();
    });

    it.each([
      ['no key is stored', () => transport.revealSecret.mockImplementation(async () => null)],
      ['the reveal fails', () => transport.revealSecret.mockImplementation(async () => Promise.reject(new Error('daemon gone')))],
    ])('Clear can be pressed when %s, and sends no key', async (_, answer) => {
      answer();
      const { user } = renderEndpoint();
      await frame();
      const clear = within(screen.getByRole('group', { name: 'Activity' })).getByRole('button', { name: 'Clear prompt cache' });
      await waitFor(() => expect(clear).toBeEnabled());
      await user.click(clear);
      expect(dashboard.clear).toHaveBeenCalledWith('127.0.0.1', 8080, null);
    });

    it('⋯ opens the live numbers and the endpoint settings', async () => {
      const { user, at } = renderEndpoint();
      await frame();
      await user.click(screen.getByRole('button', { name: 'More' }));
      await user.click(screen.getByRole('menuitem', { name: 'Live numbers' }));
      expect(screen.getByRole('dialog', { name: 'Live numbers' })).toBeInTheDocument();

      await user.click(screen.getByRole('button', { name: 'More' }));
      await user.click(screen.getByRole('menuitem', { name: 'Endpoint settings…' }));
      expect(at()).toBe('/settings/endpoint');
    });
  });

  describe('the inspector beside the place', () => {
    let clock = Date.now() + 2e9;
    afterEach(() => act(() => ingestServerEvent({ type: 'stopped', modelId: '1', updatedAt: (clock += 1e6) })));

    it('a row opens its model beside the place, and × closes it to the place', async () => {
      const { user, at } = renderEndpoint();
      await frame(admitted([residentSlot(1, 'Qwen3-4B')]));
      await user.click(loadedRow('Qwen3-4B'));
      expect(at()).toBe('/endpoint?inspect=model:1');
      const inspector = screen.getByRole('complementary', { name: 'Inspector' });
      expect(within(inspector).getByRole('heading', { name: 'Qwen3-4B' })).toBeInTheDocument();
      expect(loadedRow('Qwen3-4B')).toHaveAttribute('aria-selected', 'true');

      await user.click(within(inspector).getByRole('button', { name: 'Close' }));
      expect(at()).toBe('/endpoint');
      expect(screen.queryByRole('complementary', { name: 'Inspector' })).not.toBeInTheDocument();
    });

    it("a loaded model's status line says its latest rate and how long it has been loaded", async () => {
      act(() => ingestServerEvent({ type: 'running', modelId: '1', port: 9001, updatedAt: (clock += 1e6), modelName: 'Qwen3-4B' }));
      renderEndpoint(UP, '/endpoint?inspect=model:1');
      await frame({
        ...admitted([residentSlot(1, 'Qwen3-4B', { resident_for_secs: 250, launch: { ...residentSlot(1, 'x').launch!, state: 'working' } })]),
        // Oldest first, as the proxy sends them: the newest with a rate is said.
        recent_requests: [
          recentRequest({ model_name: 'Qwen3-4B', completion_tokens: 812, generation_ms: 24_000 }),
          recentRequest({ model_name: 'Qwen3-4B', completion_tokens: 50, generation_ms: 1_000 }),
          recentRequest({ model_name: 'Gemma-3-4B-It', completion_tokens: 10, generation_ms: 1_000 }),
          recentRequest({ model_name: 'Qwen3-4B', completion_tokens: 1, generation_ms: 0 }),
        ],
      });
      const inspector = screen.getByRole('complementary', { name: 'Inspector' });
      expect(within(inspector).getByText(`${RESIDENCY_WORDS.working} · 50 t/s · loaded 4m 10s`)).toBeInTheDocument();
    });
  });
});
