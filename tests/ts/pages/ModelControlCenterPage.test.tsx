/**
 * The Model Control Center as the thing that decides which screen is up.
 *
 * The defect this pins was that a laptop with no local models could tick
 * "use it for chat", name the far machine's model, and then have no way to
 * reach a chat screen at all: every route into `ChatPage` started from a
 * server running here, and `openChatSession` silently did nothing when the
 * model id matched no server.
 *
 * It has to be tested at the page and not at the hook. The hook can be
 * correct and the page still never call it — which is exactly the shape the
 * old bug had — so the assertion is on the rendered screen, reached the way
 * a user reaches it: open the Remote popover in the library header, name the
 * model, press the button.
 *
 * `ChatPage` itself is stubbed. It is lazily imported and drags in the whole
 * assistant-ui runtime; what is under test here is which screen the page
 * chooses and what it hands it, and the stub shows both.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { ReactNode, useState } from 'react';

// The library the page loads. Empty for the remote cases; one model for the
// case about a model already running here.
const library = vi.hoisted(() => ({ models: [] as unknown[] }));
const serveModel = vi.hoisted(() => vi.fn(async (_config: { id: number }) => ({ port: 9456 })));
// What the stub chat page's switch button picks, and the conversation it
// reports as open when the switch lands.
const stub = vi.hoisted(() => ({
  choice: { modelId: 9, modelName: 'gemma-3-12b' },
  conversationId: 2 as number | null,
  draft: 'half a thought',
}));

vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>(
    '../../../src/services/transport',
  );
  return {
    ...actual,
    getTransport: () => ({
      listModels: vi.fn(async () => library.models),
      getModelDetail: vi.fn(async () => null),
      serveModel,
      listTags: vi.fn(async () => []),
      getModelFilterOptions: vi.fn(async () => ({
        architectures: [],
        quantizations: [],
        tags: [],
        parameterSizes: [],
      })),
      getDownloadQueue: vi.fn(async () => ({ active: [], queued: [], recent: [] })),
      getSettings: vi.fn(async () => ({})),
      subscribe: vi.fn(() => () => {}),
    }),
  };
});
vi.mock('../../../src/services/transport/api/client', () => ({
  // The library list the page draws is the filtered fetch, not `listModels`;
  // a model's sampling explanation is "none", which the inspector can draw.
  get: vi.fn(async (path: string) =>
    path.startsWith('/api/models?') ? library.models : path.startsWith('/api/models/') ? null : [],
  ),
  getAuthenticatedFetchConfig: vi.fn(async () => ({ baseUrl: '', headers: {} })),
}));
vi.mock('../../../src/services/remoteEvents', () => ({
  refreshRemoteStatus: vi.fn(),
}));

// The screen under test is "which page is showing", so the chat page is a
// placard: it names the model, port and conversation it was given, says
// whether it was told the session is remote, and offers its model switch,
// and Unload when it was handed one.
// The port is held as the real page holds its session, from its first
// render, so a switch that does not remount the page shows the old one.
vi.mock('../../../src/pages/ChatPage', () => ({
  default: ({
    modelName,
    serverPort,
    conversationId,
    draft,
    startingModel,
    remote,
    onSwitchModel,
    onUnloadModel,
    onClose,
  }: {
    modelName: string;
    serverPort?: number;
    conversationId?: number | null;
    draft?: string;
    startingModel?: string | null;
    remote?: boolean;
    onSwitchModel?: (
      choice: { modelId: number; modelName: string },
      context: () => { conversationId: number | null; draft: string },
    ) => Promise<void>;
    onUnloadModel?: () => Promise<void>;
    onClose: () => void;
  }) => {
    const [mountedPort] = useState(serverPort);
    return (
      <div
        data-testid="chat-page"
        data-remote={remote ? 'yes' : 'no'}
        data-port={mountedPort}
        data-conversation={conversationId ?? ''}
        data-draft={draft ?? ''}
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
import { ConfirmProvider } from '../../../src/contexts/ConfirmContext';
import { SettingsProvider } from '../../../src/contexts/SettingsContext';
import {
  IDLE_STATUS,
  applyRemoteStatus,
  resetRemoteState,
} from '../../../src/services/remoteRegistry';

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
  has_remote_key: true,
};

const stopServer = vi.fn(async () => {});
const loadServers = vi.fn(async () => {});

/** Render the page with nothing served here — the laptop this PR is about. */
function renderPage() {
  return render(
    <ModelControlCenterPage servers={[]} loadServers={loadServers} stopServer={stopServer} />,
    { wrapper },
  );
}

/** Open the Remote popover in the library header and name a model there. */
async function askForRemoteChat(modelName: string) {
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: /remote/i }));
  await user.type(await screen.findByLabelText(/model on that machine/i), modelName);
  await user.click(screen.getByRole('button', { name: /chat on that machine/i }));
  return user;
}

describe('ModelControlCenterPage', () => {
  beforeEach(() => {
    library.models = [];
    serveModel.mockClear();
    stub.choice = { modelId: 9, modelName: 'gemma-3-12b' };
    stub.conversationId = 2;
    resetRemoteState();
    stopServer.mockClear();
    loadServers.mockClear();
  });

  it('opens the chat screen on the far machine with nothing served here', async () => {
    applyRemoteStatus(CONNECTED);
    renderPage();

    // The library is empty and the Remote popover is still reachable: the
    // header renders whatever the model count is.
    await askForRemoteChat('qwen3');

    const chat = await screen.findByTestId('chat-page');
    expect(chat).toHaveTextContent('Chatting with qwen3');
    expect(chat).toHaveAttribute('data-remote', 'yes');
  });

  it('leaves the far machine running when its chat is closed', async () => {
    applyRemoteStatus(CONNECTED);
    renderPage();
    const user = await askForRemoteChat('qwen3');
    await screen.findByTestId('chat-page');

    // There is no server here to stop, and the tunnel is not this page's to
    // tear down; nor is there a model here to unload.
    expect(screen.queryByRole('button', { name: 'Unload' })).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /close chat/i }));

    await waitFor(() => expect(screen.queryByTestId('chat-page')).not.toBeInTheDocument());
    expect(stopServer).not.toHaveBeenCalled();
  });

  it("an already-running model's Open chat opens the chat page", async () => {
    // Served before the page loaded — by the CLI, the tray, another window —
    // so no serve here ever fired the handler that opens chat by itself.
    library.models = [guiModel({ id: 7, name: 'qwen3-8b', isServing: true })];
    const running = [{ modelId: 7, modelName: 'qwen3-8b', port: 9123, status: 'running' as const }];
    render(
      <ModelControlCenterPage servers={running} loadServers={loadServers} stopServer={stopServer} />,
      { wrapper },
    );
    const user = userEvent.setup();

    await user.click(await screen.findByRole('option', { name: /qwen3-8b/i }));
    await user.click(await screen.findByRole('button', { name: /open chat/i }));

    const chat = await screen.findByTestId('chat-page');
    expect(chat).toHaveTextContent('Chatting with qwen3-8b');
    expect(chat).toHaveAttribute('data-remote', 'no');
  });

  /** One model running here, one not, and the chat open on the running one. */
  async function openChatOnQwen(extra: { modelId: number; modelName: string; port: number; status: 'running' }[] = []) {
    library.models = [guiModel({ id: 7, name: 'qwen3-8b', isServing: true }), guiModel({ id: 9, name: 'gemma-3-12b' })];
    const running = [{ modelId: 7, modelName: 'qwen3-8b', port: 9123, status: 'running' as const }, ...extra];
    render(
      <ModelControlCenterPage servers={running} loadServers={loadServers} stopServer={stopServer} />,
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
});
