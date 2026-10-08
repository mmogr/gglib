/**
 * The MCP Servers panel's row for a server gglib cannot run.
 *
 * The daemon lists a stored SSE server with the status `unsupported`. The row
 * says so and offers Remove alone: a test, a start or an edit of it would be
 * refused. A stdio server keeps every control it had.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom';

import { McpServersPanel } from '../../../src/components/McpServersPanel';
import type { McpServerInfo } from '../../../src/services/transport/types/mcp';
import { mcpServer, mcpServerInfo } from '../fixtures/mcp';

let servers: McpServerInfo[] = [];
const removeServer = vi.fn();

vi.mock('../../../src/hooks/useMcpServers', () => ({
  useMcpServers: () => ({
    servers,
    loading: false,
    error: null,
    refresh: vi.fn(),
    removeServer,
    startServer: vi.fn(),
    stopServer: vi.fn(),
  }),
}));

vi.mock('../../../src/contexts/ConfirmContext', () => ({
  useConfirmContext: () => ({ confirm: () => Promise.resolve(true) }),
}));

vi.mock('../../../src/contexts/ToastContext', () => ({
  useToastContext: () => ({ showToast: vi.fn() }),
}));

function renderPanel(listed: McpServerInfo) {
  servers = [listed];
  render(<McpServersPanel onAddServer={vi.fn()} onEditServer={vi.fn()} />);
}

const button = (name: string) => screen.queryByRole('button', { name });

beforeEach(() => {
  removeServer.mockReset();
  removeServer.mockResolvedValue(undefined);
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
});
