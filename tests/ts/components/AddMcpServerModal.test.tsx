/**
 * The add and edit form, and the kind of server it does not offer.
 *
 * The daemon refuses an SSE server, so the form shows the choice, keeps it
 * from being made, and says why. A stdio server is saved as it always was.
 */
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom';

import { AddMcpServerModal } from '../../../src/components/AddMcpServerModal';
import type { McpServerInfo } from '../../../src/services/transport/types/mcp';
import { mcpServerInfo } from '../fixtures/mcp';

function renderForm(editingServer?: McpServerInfo) {
  const onSave = vi.fn().mockResolvedValue(undefined);
  render(
    <AddMcpServerModal isOpen onClose={vi.fn()} onSave={onSave} editingServer={editingServer} />,
  );
  return onSave;
}

describe('AddMcpServerModal', () => {
  it.each([
    ['a new server', undefined],
    ['a stored server', mcpServerInfo()],
  ])('shows SSE as not supported yet and keeps it from being chosen for %s', (_, editing) => {
    renderForm(editing);
    const sse = screen.getByRole('radio', { name: 'SSE (connect to URL)' });

    expect(sse).toBeDisabled();
    expect(screen.getByRole('radio', { name: 'Stdio (spawn process)' })).toBeChecked();
    expect(
      screen.getByText('SSE servers are not supported yet. Only stdio servers can be run.'),
    ).toBeInTheDocument();

    fireEvent.click(sse);

    expect(sse).not.toBeChecked();
    expect(screen.queryByLabelText('Server URL *')).not.toBeInTheDocument();
    expect(screen.getByLabelText('Command *')).toBeInTheDocument();
  });

  it('saves a stdio server', async () => {
    const onSave = renderForm();
    const name = screen.getByLabelText('Server Name *');
    fireEvent.change(name, { target: { value: 'Files' } });
    fireEvent.change(screen.getByLabelText('Command *'), { target: { value: 'npx' } });
    fireEvent.change(screen.getByLabelText('Arguments'), { target: { value: '-y files-mcp' } });

    fireEvent.submit(name.closest('form')!);

    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1));
    expect(onSave).toHaveBeenCalledWith({
      name: 'Files',
      server_type: 'stdio',
      enabled: true,
      lifecycle: 'lazy',
      env: [],
      config: {
        command: 'npx',
        args: ['-y', 'files-mcp'],
        working_dir: undefined,
        path_extra: undefined,
      },
    });
  });
});
