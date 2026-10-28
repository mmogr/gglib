/**
 * The settings dialog's MCP tab shows what its own add and edit saved.
 *
 * The panel and the dialog each used to hold a `useMcpServers()`. A save went
 * through the dialog's, which refreshed, and the panel kept the list it had
 * loaded: a server added did not appear until Refresh, and an Edit opened
 * from the stale row sent a variable the server no longer kept. This drives
 * the real hook over a fake transport, so two copies of the list fail it.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { SettingsModal } from '../../../src/components/SettingsModal';
import { ToastProvider } from '../../../src/contexts/ToastContext';
import type { McpServerInfo, NewMcpServer } from '../../../src/services/transport/types/mcp';
import { mcpServer, mcpServerInfo } from '../fixtures/mcp';

/**
 * What the fake daemon stores, and the hooks' answers. Each is one object for
 * the whole file: a fresh one per render would re-run the dialog's effects
 * on every render, without end.
 */
const { stored, lists, settingsHook, directoryHook, modelsHook } = vi.hoisted(() => ({
  stored: [] as McpServerInfo[],
  lists: { count: 0 },
  settingsHook: {
    settings: { hasProxyApiKey: false, inferenceProfiles: [] },
    loading: false,
    saving: false,
    error: null,
    refresh: async () => {},
    save: async () => ({}),
  },
  directoryHook: {
    info: null,
    loading: false,
    saving: false,
    error: null,
    refresh: async () => {},
    save: async () => {},
  },
  modelsHook: { models: [], loading: false, error: null, refresh: async () => {} },
}));

vi.mock('../../../src/services/transport', () => ({
  getTransport: () => ({
    listMcpServers: async () => {
      lists.count += 1;
      return [...stored];
    },
    addMcpServer: async (server: NewMcpServer) => {
      const info = mcpServerInfo({ server: mcpServer({ id: stored.length + 1, name: server.name }) });
      stored.push(info);
      return info;
    },
  }),
}));

vi.mock('../../../src/hooks/useSettings', () => ({ useSettings: () => settingsHook }));
vi.mock('../../../src/hooks/useModelsDirectory', () => ({ useModelsDirectory: () => directoryHook }));
vi.mock('../../../src/hooks/useModels', () => ({ useModels: () => modelsHook }));

vi.mock('../../../src/contexts/ConfirmContext', () => ({
  useConfirmContext: () => ({ confirm: () => Promise.resolve(true) }),
}));

const openDialog = (initialTab?: 'mcp') =>
  render(
    <ToastProvider>
      <SettingsModal isOpen onClose={vi.fn()} initialTab={initialTab} />
    </ToastProvider>,
  );

describe('SettingsModal — MCP tab', () => {
  beforeEach(() => {
    stored.length = 0;
    lists.count = 0;
  });

  /** A server stored while the dialog was open on another tab, by the CLI say. */
  it('reloads the list when the tab is opened', async () => {
    const user = userEvent.setup();
    openDialog();
    await waitFor(() => expect(lists.count).toBe(1));

    stored.push(mcpServerInfo({ server: mcpServer({ id: 7, name: 'from-the-cli' }) }));
    await user.click(screen.getByRole('tab', { name: /mcp servers/i }));

    expect(await screen.findByText('from-the-cli')).toBeInTheDocument();
  });

  it('lists a server added through the dialog without a Refresh', async () => {
    const user = userEvent.setup();
    openDialog('mcp');

    await user.click(await screen.findByRole('button', { name: /add your first server/i }));
    await user.type(screen.getByPlaceholderText('My MCP Server'), 'files');
    await user.type(screen.getByPlaceholderText('npx, python3, node'), 'cat');
    await user.click(screen.getByRole('button', { name: /^add server$/i }));

    expect(await screen.findByText('files')).toBeInTheDocument();
    expect(screen.getByText(/1 server/)).toBeInTheDocument();
  });
});
