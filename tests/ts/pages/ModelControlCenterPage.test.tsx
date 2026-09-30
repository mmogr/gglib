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
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { ReactNode, useState } from 'react';

// The library the page loads. Empty for the remote cases; one model for the
// case about a model already running here.
const library = vi.hoisted(() => ({ models: [] as unknown[] }));
const serveModel = vi.hoisted(() => vi.fn(async () => ({ port: 9456 })));

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
// whether it was told the session is remote, and offers its model switch.
// The port is held as the real page holds its session, from its first
// render, so a switch that does not remount the page shows the old one.
vi.mock('../../../src/pages/ChatPage', () => ({
  default: ({
    modelName,
    serverPort,
    conversationId,
    remote,
    onSwitchModel,
    onClose,
  }: {
    modelName: string;
    serverPort?: number;
    conversationId?: number | null;
    remote?: boolean;
    onSwitchModel?: (choice: { modelId: number; modelName: string }, conversationId: number | null) => Promise<void>;
    onClose: () => Promise<void>;
  }) => {
    const [mountedPort] = useState(serverPort);
    return (
      <div
        data-testid="chat-page"
        data-remote={remote ? 'yes' : 'no'}
        data-port={mountedPort}
        data-conversation={conversationId ?? ''}
      >
        Chatting with {modelName}
        <button type="button" onClick={() => void onSwitchModel?.({ modelId: 9, modelName: 'gemma-3-12b' }, 2)}>
          Switch to gemma
        </button>
        <button type="button" onClick={() => void onClose()}>
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

    // Closing a local chat stops the server it was talking to. There is no
    // server here to stop, and the tunnel is not this page's to tear down.
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

  it('serves a model that is not running, then moves the chat to it with its conversation', async () => {
    library.models = [guiModel({ id: 7, name: 'qwen3-8b', isServing: true }), guiModel({ id: 9, name: 'gemma-3-12b' })];
    const running = [{ modelId: 7, modelName: 'qwen3-8b', port: 9123, status: 'running' as const }];
    render(
      <ModelControlCenterPage servers={running} loadServers={loadServers} stopServer={stopServer} />,
      { wrapper },
    );
    const user = userEvent.setup();
    await user.click(await screen.findByRole('option', { name: /qwen3-8b/i }));
    await user.click(await screen.findByRole('button', { name: /open chat/i }));

    await user.click(await screen.findByRole('button', { name: 'Switch to gemma' }));

    await waitFor(() => expect(screen.getByTestId('chat-page')).toHaveTextContent('Chatting with gemma-3-12b'));
    expect(serveModel).toHaveBeenCalledWith({ id: 9 });
    const chat = screen.getByTestId('chat-page');
    expect(chat).toHaveAttribute('data-port', '9456');
    expect(chat).toHaveAttribute('data-conversation', '2');
    // The model it left keeps running; only Close stops a server.
    expect(stopServer).not.toHaveBeenCalled();
  });
});
