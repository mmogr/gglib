/**
 * The add and edit form, and the kind of server it does not offer.
 *
 * The daemon refuses an SSE server, so the form shows the choice, keeps it
 * from being made, and says why. A stdio server is saved as it always was.
 * The quick-start templates offer no filesystem server: reading files is
 * built in, and the picker says so. A stored server's variables arrive as
 * keys alone, and an edit sends a kept one back without its value (ADR 0027).
 */
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom';

import { AddMcpServerModal } from '../../../src/components/AddMcpServerModal';
import { MASK } from '../../../src/components/ui/SecretField';
import { ToastProvider } from '../../../src/contexts/ToastContext';
import { SERVER_TEMPLATES } from '../../../src/components/AddMcpServerModal/ServerTemplatePicker';
import type { McpServerInfo } from '../../../src/services/transport/types/mcp';
import { mcpServerInfo } from '../fixtures/mcp';

const { revealSecret } = vi.hoisted(() => ({ revealSecret: vi.fn() }));

vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => ({ revealSecret }),
}));

function renderForm(editingServer?: McpServerInfo) {
  const onSave = vi.fn().mockResolvedValue(undefined);
  render(
    <ToastProvider>
      <AddMcpServerModal isOpen onClose={vi.fn()} onSave={onSave} editingServer={editingServer} />
    </ToastProvider>,
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

  it('offers no filesystem template and says reading files is built in', () => {
    renderForm();

    expect(screen.queryByText('Filesystem Access')).not.toBeInTheDocument();
    for (const name of ['Tavily Web Search', 'GitHub', 'Brave Search']) {
      expect(screen.getByText(name)).toBeInTheDocument();
    }
    expect(
      screen.getByText(
        'Reading files is built in: choose a folder in a chat. Every template here needs an API key.',
      ),
    ).toBeInTheDocument();
  });

  it('every template names the API key it needs', () => {
    for (const template of SERVER_TEMPLATES) {
      expect(template.envKeys.length, template.name).toBeGreaterThan(0);
    }
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

  /** A stored server with two variables, its values held by the daemon. */
  const keyed = () =>
    mcpServerInfo({
      server: { ...mcpServerInfo().server, id: 7, env_keys: ['TOKEN', 'ROOT'] },
    });

  /** Submit the form and hand back the variables it saved. */
  async function savedEnv(onSave: ReturnType<typeof vi.fn>) {
    fireEvent.submit(screen.getByLabelText('Server Name *').closest('form')!);
    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1));
    return onSave.mock.calls[0][0].env;
  }

  it('shows a stored variable by its key, read-only, with its value held', () => {
    renderForm(keyed());

    const key = screen.getByLabelText('TOKEN, kept');
    expect(key).toHaveValue('TOKEN');
    expect(key).toHaveAttribute('readonly');
    expect(screen.getByLabelText('TOKEN value')).toHaveValue(MASK);
  });

  it('sends a kept variable back without its value', async () => {
    const onSave = renderForm(keyed());

    expect(await savedEnv(onSave)).toEqual([{ key: 'TOKEN' }, { key: 'ROOT' }]);
  });

  it('sends a replaced value, and leaves a removed variable out', async () => {
    const onSave = renderForm(keyed());
    fireEvent.click(screen.getAllByRole('button', { name: 'Replace' })[0]);
    fireEvent.change(screen.getByLabelText('TOKEN value'), { target: { value: 'rotated' } });
    fireEvent.click(screen.getByRole('button', { name: 'Remove ROOT' }));

    expect(await savedEnv(onSave)).toEqual([{ key: 'TOKEN', value: 'rotated' }]);
  });

  it('sends a variable typed here with its key and value', async () => {
    const onSave = renderForm(keyed());
    fireEvent.click(screen.getByRole('button', { name: /add variable/i }));
    fireEvent.change(screen.getByLabelText('Variable name'), { target: { value: 'NEW' } });
    fireEvent.change(screen.getByLabelText('NEW value'), { target: { value: 'v' } });

    expect(await savedEnv(onSave)).toEqual([
      { key: 'TOKEN' },
      { key: 'ROOT' },
      { key: 'NEW', value: 'v' },
    ]);
  });

  /** Removing a row must not hand its field, and a value shown in it, on. */
  it('shows the next variable masked after a revealed one is removed', async () => {
    revealSecret.mockResolvedValue('t0k-81d0e4');
    renderForm(keyed());
    fireEvent.click(screen.getByRole('button', { name: 'Show the TOKEN value' }));
    await waitFor(() => expect(screen.getByLabelText('TOKEN value')).toHaveValue('t0k-81d0e4'));

    fireEvent.click(screen.getByRole('button', { name: 'Remove TOKEN' }));

    expect(screen.getByLabelText('ROOT value')).toHaveValue(MASK);
    expect(screen.queryByDisplayValue('t0k-81d0e4')).not.toBeInTheDocument();
  });

  it('types a new variable into a masked input with an eye', () => {
    renderForm();
    fireEvent.click(screen.getByRole('button', { name: /add variable/i }));

    expect(screen.getByLabelText('New variable value')).toHaveAttribute('type', 'password');
    expect(screen.getByRole('button', { name: 'Show the typed New variable value' })).toBeInTheDocument();
  });
});
