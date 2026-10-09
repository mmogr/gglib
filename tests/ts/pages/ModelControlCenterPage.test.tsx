/**
 * The Model Control Center as the thing that decides which screen is up.
 *
 * A laptop with no local models must reach a chat with the paired machine's
 * model: every other route into `ChatPage` starts from a server running
 * here, and `openChatSession` does nothing when the model id matches no
 * server. So the paired machine's models are rows of the library, after this
 * machine's, and a far row's inspector opens the chat.
 *
 * It has to be tested at the page and not at the hook. The hook can be
 * correct and the page still never call it, so the assertion is on the
 * rendered screen, reached the way a user reaches it: pick the far row, press
 * Chat. The one selection across both machines is pinned here too, and that
 * a far read that never answers leaves this machine's rows working.
 *
 * Downloads are the daemon's, so the page waits on nothing from the desktop
 * shell before it offers them: one case queues a download in a desktop
 * window whose shell says nothing.
 *
 * `ChatPage` itself is stubbed. It is lazily imported and drags in the whole
 * assistant-ui runtime; what is under test here is which screen the page
 * chooses and what it hands it, and the stub shows both.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { ReactNode, useState } from 'react';

// The library the page loads. Empty for the paired cases; one model for the
// case about a model already running here.
const library = vi.hoisted(() => ({ models: [] as unknown[] }));
// The paired machine's models, and whether reading them hangs.
const far = vi.hoisted(() => ({ hang: false }));
// This machine's model as its detail route answers, the projector and
// component files its pickers are offered, and the update the page sends.
const here = vi.hoisted(() => ({ detail: null as unknown, projectors: null as unknown, components: null as unknown }));
// The download queue the daemon answers with.
const downloads = vi.hoisted(() => ({ queue: null as unknown }));
const updateModel = vi.hoisted(() =>
  vi.fn(async (_params: { id: number; projectorPath?: string | null; components?: Record<string, string | null> }) => ({})),
);
const queueDownload = vi.hoisted(() => vi.fn(async (_params: { modelId: string; quantization?: string }) => ({ id: 'q-1' })));
// The desktop shell's event channel: it records who listens and sends nothing.
const shell = vi.hoisted(() => ({ listen: vi.fn(async (_event: string) => () => {}) }));
const serveModel = vi.hoisted(() => vi.fn(async (_config: { id: number }) => ({ port: 9456 })));
// What the stub chat page's switch button picks, and the conversation it
// reports as open when the switch lands.
const stub = vi.hoisted(() => ({
  choice: { modelId: 9, modelName: 'gemma-3-12b' },
  conversationId: 2 as number | null,
  draft: { text: 'half a thought', images: [new File(['png'], 'shot.png', { type: 'image/png' })] },
}));

vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>(
    '../../../src/services/transport',
  );
  return {
    ...actual,
    getTransport: () => ({
      listModels: vi.fn(async () => library.models),
      getModelDetail: vi.fn(async () => here.detail),
      updateModel,
      listPairedModels: vi.fn(() =>
        far.hang ? new Promise(() => {}) : Promise.resolve(pairedModels([farEntry('qwen3-8b', 7)])),
      ),
      getPairedModel: vi.fn(async (id: number) => ({ detail: farDetail(id, 'qwen3-8b') })),
      loadPairedModel: vi.fn(async () => ({ model: 'qwen3-8b', started: true, context: 30000 })),
      serveModel,
      listTags: vi.fn(async () => []),
      getModelFilterOptions: vi.fn(async () => ({
        architectures: [],
        quantizations: [],
        tags: [],
        parameterSizes: [],
      })),
      getDownloadQueue: vi.fn(async () => downloads.queue),
      queueDownload,
      browseHfModels: vi.fn(async () => ({ models: [], has_more: false, page: 0 })),
      getSettings: vi.fn(async () => ({})),
      subscribe: vi.fn(() => () => {}),
      onEventStreamOpen: vi.fn(() => () => {}),
    }),
  };
});
vi.mock('../../../src/services/transport/api/client', () => ({
  // A model's sampling explanation is "none", which the inspector can draw,
  // and so is the model suggested for this machine.
  get: vi.fn(async (path: string) =>
    path.endsWith('/projectors')
      ? here.projectors
      : path.endsWith('/components')
        ? here.components
        : path.startsWith('/api/models/') || path.endsWith('/recommend-model')
        ? null
        : [],
  ),
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: shell.listen }));
vi.mock('../../../src/services/remoteEvents', () => ({
  refreshRemoteStatus: vi.fn(),
}));

// The screen under test is "which page is showing", so the chat page is a
// placard: it names the model, port and conversation it was given, says
// which machine a paired session is on, and offers its model switch, and
// Unload when it was handed one.
// The port is held as the real page holds its session, from its first
// render, so a switch that does not remount the page shows the old one.
vi.mock('../../../src/pages/ChatPage', () => ({
  default: ({
    modelName,
    serverPort,
    conversationId,
    draft,
    startingModel,
    paired,
    onSwitchModel,
    onUnloadModel,
    onClose,
  }: {
    modelName: string;
    serverPort?: number;
    conversationId?: number | null;
    draft?: { text: string; images: File[] };
    startingModel?: string | null;
    paired?: { far: { id: number }; machineName: string };
    onSwitchModel?: (
      choice: { modelId: number; modelName: string },
      context: () => { conversationId: number | null; draft: { text: string; images: File[] } },
    ) => Promise<void>;
    onUnloadModel?: () => Promise<void>;
    onClose: () => void;
  }) => {
    const [mountedPort] = useState(serverPort);
    return (
      <div
        data-testid="chat-page"
        data-paired={paired ? `${paired.machineName}:${paired.far.id}` : ''}
        data-port={mountedPort}
        data-conversation={conversationId ?? ''}
        data-draft={draft?.text ?? ''}
        data-draft-images={draft?.images.map((f) => f.name).join(',') ?? ''}
        data-starting={startingModel ?? ''}
      >
        Chatting with {modelName}
        <button
          type="button"
          // The real picker toasts a failure; the stub only swallows it.
          onClick={() =>
            void onSwitchModel?.(stub.choice, () => ({ conversationId: stub.conversationId, draft: stub.draft }))?.catch(() => {})
          }
        >
          Switch model
        </button>
        {onUnloadModel && (
          <button type="button" onClick={() => void onUnloadModel()}>
            Unload
          </button>
        )}
        <button type="button" onClick={onClose}>
          Close chat
        </button>
      </div>
    );
  },
}));

import ModelControlCenterPage from '../../../src/pages/ModelControlCenterPage';
import { ToastProvider } from '../../../src/contexts/ToastContext';
import { guiModel } from '../fixtures/model';
import { farDetail, farEntry, pairedModels } from '../fixtures/fakeFarDaemon';
import { queueSnapshot, runningRow, waitingRow } from '../fixtures/downloads';
import { ConfirmProvider } from '../../../src/contexts/ConfirmContext';
import { SettingsProvider } from '../../../src/contexts/SettingsContext';
import {
  IDLE_STATUS,
  applyRemoteStatus,
  resetRemoteState,
} from '../../../src/services/remoteRegistry';
import { get } from '../../../src/services/transport/api/client';

const wrapper = ({ children }: { children: ReactNode }) => (
  <ToastProvider>
    <ConfirmProvider>
      <SettingsProvider showToast={() => {}}>{children}</SettingsProvider>
    </ConfirmProvider>
  </ToastProvider>
);

const CONNECTED = {
  ...IDLE_STATUS,
  connected: {
    port: 41234,
    base_url: 'http://127.0.0.1:41234/v1',
    ticket_fingerprint: '3ca82708b995',
    path: 'direct' as const,
    away_for_s: null,
  },
  stored_ticket_fingerprint: '3ca82708b995',
  paired_name: 'desk',
  has_remote_key: true,
};

const stopServer = vi.fn(async () => {});

/** Render the page with nothing served here — the laptop this PR is about. */
function renderPage() {
  return render(
    <ModelControlCenterPage servers={[]} stopServer={stopServer} />,
    { wrapper },
  );
}

/** Pick the paired machine's row for `name`, under that machine's group. */
async function pickFar(name: RegExp) {
  const user = userEvent.setup();
  const group = await screen.findByRole('listbox', { name: "desk's models" });
  await user.click(within(group).getByRole('option', { name }));
  return user;
}

describe('ModelControlCenterPage', () => {
  beforeEach(() => {
    library.models = [];
    downloads.queue = queueSnapshot();
    here.detail = null;
    here.projectors = null;
    here.components = null;
    updateModel.mockClear();
    queueDownload.mockClear();
    shell.listen.mockClear();
    serveModel.mockClear();
    stub.choice = { modelId: 9, modelName: 'gemma-3-12b' };
    stub.conversationId = 2;
    far.hang = false;
    resetRemoteState();
    stopServer.mockClear();
  });

  it("opens a chat with the paired machine's model with nothing served here", async () => {
    applyRemoteStatus(CONNECTED);
    renderPage();

    // The library is empty here; the paired machine's rows follow it.
    const user = await pickFar(/qwen3-8b/);
    expect(await screen.findByText('gglib chat 7 --remote')).toBeInTheDocument();
    await user.click(await screen.findByRole('button', { name: 'Chat' }));

    const chat = await screen.findByTestId('chat-page');
    expect(chat).toHaveTextContent('Chatting with qwen3-8b');
    expect(chat).toHaveAttribute('data-paired', 'desk:7');
  });

  it('leaves the far machine running when its chat is closed', async () => {
    applyRemoteStatus(CONNECTED);
    renderPage();
    const user = await pickFar(/qwen3-8b/);
    await user.click(await screen.findByRole('button', { name: 'Chat' }));
    await screen.findByTestId('chat-page');

    // There is no server here to stop, and the tunnel is not this page's to
    // tear down; nor is there a model here to unload.
    expect(screen.queryByRole('button', { name: 'Unload' })).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /close chat/i }));

    await waitFor(() => expect(screen.queryByTestId('chat-page')).not.toBeInTheDocument());
    expect(stopServer).not.toHaveBeenCalled();
  });

  it('the same model on both machines is two rows, and picking one clears the other', async () => {
    library.models = [guiModel({ id: 7, name: 'qwen3-8b' })];
    applyRemoteStatus(CONNECTED);
    const keys = vi.spyOn(console, 'error');
    renderPage();
    const user = userEvent.setup();

    const here = await screen.findByRole('listbox', { name: 'Model library' });
    await user.click(within(here).getByRole('option', { name: /qwen3-8b/ }));
    expect(within(here).getByRole('option', { name: /qwen3-8b/ })).toHaveAttribute('aria-selected', 'true');

    // The far row is its own row, badged with its machine.
    const there = await screen.findByRole('listbox', { name: "desk's models" });
    const farRow = within(there).getByRole('option', { name: /qwen3-8b/ });
    expect(farRow).toHaveTextContent('desk');
    await user.click(farRow);
    expect(farRow).toHaveAttribute('aria-selected', 'true');
    expect(within(here).getByRole('option', { name: /qwen3-8b/ })).toHaveAttribute('aria-selected', 'false');
    // The far inspector offers what that machine allows and nothing that changes it.
    expect(await screen.findByRole('button', { name: 'Chat' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /delete/i })).not.toBeInTheDocument();

    await user.click(within(here).getByRole('option', { name: /qwen3-8b/ }));
    expect(farRow).toHaveAttribute('aria-selected', 'false');
    expect(screen.queryByText('gglib chat 7 --remote')).not.toBeInTheDocument();
    expect(keys.mock.calls.flat().join(' ')).not.toMatch(/same key/);
    keys.mockRestore();
  });

  it("a far read that never answers leaves this machine's rows working", async () => {
    library.models = [guiModel({ id: 7, name: 'qwen3-8b' })];
    far.hang = true;
    applyRemoteStatus(CONNECTED);
    renderPage();
    const user = userEvent.setup();

    await user.click(await screen.findByRole('option', { name: /qwen3-8b/ }));

    expect(screen.getByRole('option', { name: /qwen3-8b/ })).toHaveAttribute('aria-selected', 'true');
    expect(screen.queryByRole('listbox', { name: "desk's models" })).not.toBeInTheDocument();
  });

  it("an already-running model's Open chat opens the chat page", async () => {
    // Served before the page loaded — by the CLI, the tray, another window —
    // so no serve here ever fired the handler that opens chat by itself.
    library.models = [guiModel({ id: 7, name: 'qwen3-8b', isServing: true })];
    const running = [{ modelId: 7, modelName: 'qwen3-8b', port: 9123, status: 'running' as const }];
    render(
      <ModelControlCenterPage servers={running} stopServer={stopServer} />,
      { wrapper },
    );
    const user = userEvent.setup();

    await user.click(await screen.findByRole('option', { name: /qwen3-8b/i }));
    await user.click(await screen.findByRole('button', { name: /open chat/i }));

    const chat = await screen.findByTestId('chat-page');
    expect(chat).toHaveTextContent('Chatting with qwen3-8b');
    expect(chat).toHaveAttribute('data-paired', '');
  });

  it('draws the download the queue read at load names, with no event yet', async () => {
    // A page opened mid-download: the stream has sent nothing, and the row
    // the read returned is on screen in its own words.
    downloads.queue = queueSnapshot({
      revision: 12,
      active: runningRow(),
      waiting: [waitingRow('owner/b:Q4_K_M', 2), waitingRow('owner/c:Q4_K_M', 3)],
    });
    renderPage();

    expect(await screen.findByText('owner/zeta-GGUF:Q8_0')).toBeInTheDocument();
    expect(screen.getByText('7.00 GiB / 28.00 GiB')).toBeInTheDocument();
    expect(screen.getByText('118.4 MB/s')).toBeInTheDocument();
    expect(screen.getByText('+2 queued')).toBeInTheDocument();
  });

  /** One model running here, one not, and the chat open on the running one. */
  async function openChatOnQwen(extra: { modelId: number; modelName: string; port: number; status: 'running' }[] = []) {
    library.models = [guiModel({ id: 7, name: 'qwen3-8b', isServing: true }), guiModel({ id: 9, name: 'gemma-3-12b' })];
    const running = [{ modelId: 7, modelName: 'qwen3-8b', port: 9123, status: 'running' as const }, ...extra];
    render(
      <ModelControlCenterPage servers={running} stopServer={stopServer} />,
      { wrapper },
    );
    const user = userEvent.setup();
    await user.click(await screen.findByRole('option', { name: /qwen3-8b/i }));
    await user.click(await screen.findByRole('button', { name: /open chat/i }));
    await screen.findByTestId('chat-page');
    return user;
  }

  /** A serve that answers when the test says so. */
  function heldServe() {
    let answer!: (value: { port: number }) => void;
    serveModel.mockImplementationOnce(() => new Promise((resolve) => { answer = resolve; }));
    return (port: number) => act(async () => answer({ port }));
  }

  it('offers Open chat only for a model that is running', async () => {
    library.models = [guiModel({ id: 9, name: 'gemma-3-12b' })];
    renderPage();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('option', { name: /gemma-3-12b/i }));

    expect(await screen.findByRole('button', { name: /start endpoint/i })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /open chat/i })).not.toBeInTheDocument();
  });

  it("links this machine's model to the projector picked in its inspector, by the library's update", async () => {
    library.models = [guiModel({ id: 9, name: 'gemma-3-12b' })];
    here.detail = farDetail(9, 'gemma-3-12b', { filePath: '/models/g/gemma.gguf' });
    here.projectors = [{ path: '/models/g/mmproj-gemma.gguf', name: 'mmproj-gemma.gguf' }];
    renderPage();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('option', { name: /gemma-3-12b/i }));

    const picker = await screen.findByRole('combobox', { name: 'Projector' });
    await waitFor(() => expect(picker).toBeEnabled());
    await user.selectOptions(picker, await within(picker).findByRole('option', { name: 'mmproj-gemma.gguf' }));

    await waitFor(() => expect(updateModel).toHaveBeenCalledTimes(1));
    expect(updateModel.mock.calls[0][0]).toMatchObject({ id: 9, projectorPath: '/models/g/mmproj-gemma.gguf' });
  });

  it("marks this machine's model that reads images in its library row and its inspector", async () => {
    library.models = [guiModel({ id: 9, name: 'gemma-3-12b', imageInput: true })];
    renderPage();
    const row = await screen.findByRole('option', { name: /gemma-3-12b/i });
    expect(within(row).getByText('Vision')).toBeInTheDocument();

    await userEvent.setup().click(row);

    const heading = await screen.findByRole('heading', { name: 'gemma-3-12b' });
    expect(within(heading.parentElement!).getByText('Vision')).toBeInTheDocument();
  });

  it("links this machine's image model's component picked in its inspector, by the library's update", async () => {
    library.models = [guiModel({ id: 9, name: 'flux1-schnell', imageFamily: 'flux1', missingComponents: ['vae'] })];
    here.detail = farDetail(9, 'flux1-schnell', {
      filePath: '/models/f/flux1-schnell-q8_0.gguf',
      imageFamily: 'flux1',
      missingComponents: ['vae'],
    });
    here.components = [{ role: 'vae', files: [{ path: '/models/u/ae.safetensors', name: 'ae.safetensors' }] }];
    renderPage();
    const user = userEvent.setup();
    const row = await screen.findByRole('option', { name: /flux1-schnell/i });
    expect(within(row).getByText('Draws · Flux.1')).toBeInTheDocument();
    await user.click(row);

    const picker = await screen.findByRole('combobox', { name: 'VAE' });
    await waitFor(() => expect(picker).toBeEnabled());
    await user.selectOptions(picker, await within(picker).findByRole('option', { name: 'ae.safetensors' }));

    await waitFor(() => expect(updateModel).toHaveBeenCalledTimes(1));
    expect(updateModel.mock.calls[0][0]).toMatchObject({ id: 9, components: { vae: '/models/u/ae.safetensors' } });
  });

  it("shows no Components row for this machine's model that chats, and reads no component choices for it", async () => {
    library.models = [guiModel({ id: 9, name: 'gemma-3-12b' })];
    here.detail = farDetail(9, 'gemma-3-12b', { filePath: '/models/g/gemma.gguf' });
    vi.mocked(get).mockClear();
    renderPage();
    await userEvent.setup().click(await screen.findByRole('option', { name: /gemma-3-12b/i }));

    await screen.findByRole('combobox', { name: 'Projector' });
    await waitFor(() => expect(vi.mocked(get).mock.calls.map(([path]) => path)).toContain('/api/models/9/projectors'));
    expect(screen.queryByText('Components')).not.toBeInTheDocument();
    expect(vi.mocked(get).mock.calls.map(([path]) => path)).not.toContain('/api/models/9/components');
  });

  it('leaves the model loaded when a local chat is closed', async () => {
    // The proxy serves it to every client, Copilot included (#1211).
    const user = await openChatOnQwen();

    await user.click(screen.getByRole('button', { name: 'Close chat' }));

    await waitFor(() => expect(screen.queryByTestId('chat-page')).not.toBeInTheDocument());
    expect(stopServer).not.toHaveBeenCalled();
  });

  it("unloads the chat's model and leaves the chat open", async () => {
    const user = await openChatOnQwen();

    await user.click(screen.getByRole('button', { name: 'Unload' }));

    await waitFor(() => expect(stopServer).toHaveBeenCalledWith(7));
    expect(stopServer).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId('chat-page')).toHaveTextContent('Chatting with qwen3-8b');
  });

  it('serves a model that is not running, then moves the chat to it with its conversation', async () => {
    const user = await openChatOnQwen();

    await user.click(screen.getByRole('button', { name: 'Switch model' }));

    await waitFor(() => expect(screen.getByTestId('chat-page')).toHaveTextContent('Chatting with gemma-3-12b'));
    expect(serveModel).toHaveBeenCalledWith({ id: 9 });
    const chat = screen.getByTestId('chat-page');
    expect(chat).toHaveAttribute('data-port', '9456');
    expect(chat).toHaveAttribute('data-conversation', '2');
    expect(chat).toHaveAttribute('data-draft', 'half a thought');
    expect(chat).toHaveAttribute('data-draft-images', 'shot.png');
    // The model it left keeps running; only Unload stops a server.
    expect(stopServer).not.toHaveBeenCalled();
  });

  it('moves the chat to a server already running without starting anything', async () => {
    stub.choice = { modelId: 8, modelName: 'llama-3.2-3b' };
    const user = await openChatOnQwen([{ modelId: 8, modelName: 'llama-3.2-3b', port: 9200, status: 'running' }]);

    await user.click(screen.getByRole('button', { name: 'Switch model' }));

    await waitFor(() => expect(screen.getByTestId('chat-page')).toHaveTextContent('Chatting with llama-3.2-3b'));
    expect(screen.getByTestId('chat-page')).toHaveAttribute('data-port', '9200');
    expect(serveModel).not.toHaveBeenCalled();
  });

  it('stays closed when the chat is closed while the new model starts', async () => {
    const answer = heldServe();
    const user = await openChatOnQwen();

    await user.click(screen.getByRole('button', { name: 'Switch model' }));
    await user.click(screen.getByRole('button', { name: 'Close chat' }));
    await waitFor(() => expect(screen.queryByTestId('chat-page')).not.toBeInTheDocument());
    expect(stopServer).not.toHaveBeenCalled();

    await answer(9456);

    expect(screen.queryByTestId('chat-page')).not.toBeInTheDocument();
  });

  it('opens the conversation chosen while the new model started, not the one open at the pick', async () => {
    const answer = heldServe();
    const user = await openChatOnQwen();

    await user.click(screen.getByRole('button', { name: 'Switch model' }));
    stub.conversationId = 3;
    await answer(9456);

    await waitFor(() => expect(screen.getByTestId('chat-page')).toHaveTextContent('Chatting with gemma-3-12b'));
    expect(screen.getByTestId('chat-page')).toHaveAttribute('data-conversation', '3');
  });

  it('stays on the model it was on when the new one will not start', async () => {
    serveModel.mockRejectedValueOnce(new Error('not enough memory'));
    const user = await openChatOnQwen();

    await user.click(screen.getByRole('button', { name: 'Switch model' }));

    await waitFor(() => expect(serveModel).toHaveBeenCalledTimes(1));
    const chat = screen.getByTestId('chat-page');
    expect(chat).toHaveTextContent('Chatting with qwen3-8b');
    expect(chat).toHaveAttribute('data-port', '9123');
  });

  it('lands the last model picked, whichever starts first', async () => {
    for (const order of [['first', 'second'], ['second', 'first']] as const) {
      const answer = { first: heldServe(), second: heldServe() };
      const user = await openChatOnQwen();
      await user.click(screen.getByRole('button', { name: 'Switch model' }));
      stub.choice = { modelId: 11, modelName: 'phi-4' };
      await user.click(screen.getByRole('button', { name: 'Switch model' }));
      expect(screen.getByTestId('chat-page')).toHaveAttribute('data-starting', 'phi-4');

      await answer[order[0]](order[0] === 'first' ? 9456 : 9457);
      if (order[0] === 'first') {
        // The superseded model is up, and the chat still waits on the last pick.
        expect(screen.getByTestId('chat-page')).toHaveTextContent('Chatting with qwen3-8b');
        expect(screen.getByTestId('chat-page')).toHaveAttribute('data-starting', 'phi-4');
      }
      await answer[order[1]](order[1] === 'first' ? 9456 : 9457);

      const chat = screen.getByTestId('chat-page');
      expect(chat, `resolved ${order.join(' then ')}`).toHaveTextContent('Chatting with phi-4');
      expect(chat).toHaveAttribute('data-port', '9457');
      expect(chat).toHaveAttribute('data-starting', '');
      cleanup();
      stub.choice = { modelId: 9, modelName: 'gemma-3-12b' };
    }
  });

  it('does not hand a switch to a chat reopened on the same model while it was pending', async () => {
    const answer = heldServe();
    const user = await openChatOnQwen();
    const first = screen.getByTestId('chat-page');

    await user.click(screen.getByRole('button', { name: 'Switch model' }));
    await user.click(screen.getByRole('button', { name: 'Close chat' }));
    await user.click(await screen.findByRole('button', { name: /open chat/i }));
    const reopened = await screen.findByTestId('chat-page');
    expect(reopened).not.toBe(first);
    expect(reopened).toHaveAttribute('data-starting', '');

    await answer(9456);

    expect(screen.getByTestId('chat-page')).toHaveTextContent('Chatting with qwen3-8b');
    expect(screen.getByTestId('chat-page')).toHaveAttribute('data-port', '9123');
  });

  it('tells the chat page which model is starting, until the switch lands or fails', async () => {
    const answer = heldServe();
    const user = await openChatOnQwen();

    await user.click(screen.getByRole('button', { name: 'Switch model' }));
    expect(screen.getByTestId('chat-page')).toHaveAttribute('data-starting', 'gemma-3-12b');
    await answer(9456);
    await waitFor(() => expect(screen.getByTestId('chat-page')).toHaveTextContent('Chatting with gemma-3-12b'));
    expect(screen.getByTestId('chat-page')).toHaveAttribute('data-starting', '');

    serveModel.mockRejectedValueOnce(new Error('not enough memory'));
    stub.choice = { modelId: 11, modelName: 'phi-4' };
    await user.click(screen.getByRole('button', { name: 'Switch model' }));
    await waitFor(() => expect(screen.getByTestId('chat-page')).toHaveAttribute('data-starting', ''));
    expect(screen.getByTestId('chat-page')).toHaveTextContent('Chatting with gemma-3-12b');
  });

  it('queues a download in the desktop window with no word from its shell', async () => {
    // What makes a window the desktop app's: the shell's bridge.
    Object.assign(window, { __TAURI_INTERNALS__: { invoke: vi.fn(async () => null) } });
    try {
      renderPage();
      const user = userEvent.setup();

      await user.click(await screen.findByRole('tab', { name: 'Add Models' }));
      await user.type(await screen.findByPlaceholderText(/user\/repo:quant/), 'owner/zeta-GGUF:Q8_0');
      await user.click(screen.getByRole('button', { name: 'Download' }));

      await waitFor(() =>
        expect(queueDownload).toHaveBeenCalledWith({ modelId: 'owner/zeta-GGUF', quantization: 'Q8_0' }),
      );
      // The page asks the shell for no event about downloads.
      const heard = shell.listen.mock.calls.map(([event]) => event);
      expect(heard.filter((event) => event.startsWith('download'))).toEqual([]);
    } finally {
      delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    }
  });
});
