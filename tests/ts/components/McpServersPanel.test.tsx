/**
 * The MCP Servers panel's row for a server gglib cannot run, and the box that
 * opens a row's server to paired devices.
 *
 * The daemon lists a stored SSE server with the status `unsupported`. The row
 * says so and offers Remove alone: a test, a start or an edit of it would be
 * refused. A stdio server keeps every control it had, and is the one kind a
 * row offers to open to paired devices.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom';

import { McpServersPanel } from '../../../src/components/McpServersPanel';
import type { McpServerInfo } from '../../../src/services/transport/types/mcp';
import { mcpServer, mcpServerInfo } from '../fixtures/mcp';

let servers: McpServerInfo[] = [];
const removeServer = vi.fn();
const setAllowRemote = vi.fn();

vi.mock('../../../src/hooks/useMcpServers', () => ({
  useMcpServers: () => ({
    servers,
    loading: false,
    error: null,
    refresh: vi.fn(),
    removeServer,
    startServer: vi.fn(),
    stopServer: vi.fn(),
    setAllowRemote,
  }),
}));

vi.mock('../../../src/contexts/ConfirmContext', () => ({
  useConfirmContext: () => ({ confirm: () => Promise.resolve(true) }),
}));

vi.mock('../../../src/contexts/ToastContext', () => ({
  useToastContext: () => ({ showToast: vi.fn() }),
}));

function renderPanel(...listed: McpServerInfo[]) {
  servers = listed;
  render(<McpServersPanel onAddServer={vi.fn()} onEditServer={vi.fn()} />);
}

const button = (name: string) => screen.queryByRole('button', { name });

const pairedDevices = () => screen.queryAllByRole('checkbox', { name: /Paired devices/ });

beforeEach(() => {
  removeServer.mockReset();
  removeServer.mockResolvedValue(undefined);
  setAllowRemote.mockReset();
  setAllowRemote.mockResolvedValue(undefined);
});

describe('McpServersPanel', () => {
  it('lists an SSE server as not supported yet and offers only to remove it', async () => {
    const remote = mcpServer({
      id: 7,
      name: 'Remote',
      server_type: 'sse',
      config: { url: 'http://localhost:3001/sse' },
    });
    renderPanel(mcpServerInfo({ server: remote, status: 'unsupported' }));

    expect(screen.getByText('Not supported yet')).toBeInTheDocument();
    for (const name of ['Test', 'Start', 'Stop', 'Edit', 'Auto-fix']) {
      expect(button(name)).not.toBeInTheDocument();
    }

    fireEvent.click(screen.getByRole('button', { name: 'Remove' }));

    await waitFor(() => expect(removeServer).toHaveBeenCalledWith(7));
  });

  it('offers a stdio server its test, start, edit and remove', () => {
    renderPanel(mcpServerInfo());

    expect(screen.queryByText('Not supported yet')).not.toBeInTheDocument();
    for (const name of ['Test', 'Start', 'Edit', 'Remove']) {
      expect(button(name)).toBeInTheDocument();
    }
  });

  it('offers to open a stdio server to paired devices, and never an SSE server or builtin', () => {
    const remote = mcpServer({
      id: 7,
      name: 'Remote',
      server_type: 'sse',
      config: { url: 'http://localhost:3001/sse' },
    });
    renderPanel(
      mcpServerInfo({ server: mcpServer({ id: 1, name: 'tavily', allow_remote: true }) }),
      mcpServerInfo({ server: remote, status: 'unsupported' }),
      mcpServerInfo({ server: mcpServer({ id: 9, name: 'builtin' }) }),
    );

    const boxes = pairedDevices();
    expect(boxes).toHaveLength(1);
    expect(boxes[0]).toBeChecked();
  });

  it('a tick opens the server, and a refusal is said beside the box', async () => {
    renderPanel(mcpServerInfo({ server: mcpServer({ id: 3, name: 'tavily' }) }));
    expect(pairedDevices()[0]).not.toBeChecked();

    fireEvent.click(pairedDevices()[0]);
    await waitFor(() => expect(setAllowRemote).toHaveBeenCalledWith(3, true));

    setAllowRemote.mockRejectedValueOnce(new Error('the daemon said no'));
    fireEvent.click(pairedDevices()[0]);
    expect(await screen.findByText('the daemon said no')).toBeInTheDocument();
  });

  it('says a disabled server offers paired devices no tools', () => {
    renderPanel(mcpServerInfo({ server: mcpServer({ enabled: false, allow_remote: true }) }));

    expect(screen.getByText('Disabled, so it offers no tools')).toBeInTheDocument();
  });
});
