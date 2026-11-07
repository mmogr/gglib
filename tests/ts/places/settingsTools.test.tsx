/**
 * Settings › Tools through the real routes: a tool server row's name, state
 * word and tool count; its hover actions, each greyed with why where it
 * cannot act, and Test's tools or failure under that row alone; Needs
 * relink with Fix; an opened row's command, tools and Devices may use it;
 * the one sheet that adds and edits, each field reaching the daemon under
 * its wire name, variables held masked, ▸ More, the quick starts and no SSE
 * choice; Remove… asked first; one list for the rows and the sheet (ST-8);
 * and the agent limits under Tools.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

vi.mock('../../../src/services/transport', async (importOriginal) => {
  const { transport } = await import('./settingsDaemon');
  return { ...(await importOriginal<Record<string, unknown>>()), getTransport: () => transport };
});
vi.mock('../../../src/services/transport/api/settings', async () => {
  const { transport } = await import('./settingsDaemon');
  return { getSettings: transport.getSettings, updateSettings: transport.updateSettings, installProfileTemplates: vi.fn() };
});
vi.mock('../../../src/services/tools', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  syncAllMcpTools: vi.fn(async () => {}),
}));
vi.mock('../../../src/places/library/LibraryPlace', () => ({ default: () => <main aria-label="Library" /> }));

import { MASK } from '../../../src/components/ui/SecretField';
import { SERVER_TEMPLATES } from '../../../src/places/settings/tools/TemplatePicker';
import type { McpServer, McpServerInfo } from '../../../src/services/transport/types/mcp';
import { answerTest, daemon, resetDaemon, transport } from './settingsDaemon';
import { renderApp } from './harness';
import { mcpServer, mcpServerInfo } from '../fixtures/mcp';

const tavily = (server: Partial<McpServer> = {}, info: Partial<McpServerInfo> = {}) =>
  mcpServerInfo({
    server: mcpServer({ id: 3, name: 'tavily', config: { command: 'npx', args: ['-y', 'tavily-mcp'] }, ...server }),
    ...info,
  });
const sse = () =>
  mcpServerInfo({
    server: mcpServer({ id: 7, name: 'remote', server_type: 'sse', config: { url: 'http://localhost:3001/sse' } }),
    status: 'unsupported',
  });
const tools = [
  { name: 'search', description: 'Search the web', input_schema: null },
  { name: 'extract', description: null, input_schema: null },
];

/** Open Settings › Tools over `servers` and wait for the list. */
async function openTools(...servers: McpServerInfo[]) {
  daemon.servers.push(...servers);
  renderApp('/settings/tools');
  const user = userEvent.setup();
  if (servers.length > 0) await screen.findByRole('listitem', { name: servers[0].server.name });
  else await screen.findByText('No tool servers yet. Add one to give chats tools such as web search.');
  return user;
}

const row = (name: string) => within(screen.getByRole('listitem', { name }));
const sheet = () => within(screen.getByRole('dialog'));
/** The row's name, the button that opens it. */
const opener = (name: string) => row(name).getByRole('button', { name: new RegExp(`^${name}`) });

/** Open `name`'s ⋯ and hand back its Remove… item. */
async function removeItem(user: ReturnType<typeof userEvent.setup>, name: string) {
  await user.click(row(name).getByRole('button', { name: `More for ${name}` }));
  return row(name).getByRole('menuitem', { name: 'Remove…' });
}

beforeEach(() => resetDaemon());

describe('a row', () => {
  it('says each state in its word, the tools only while running, and a failure\'s reason', async () => {
    await openTools(
      tavily({}, { status: 'running', tools }),
      mcpServerInfo({ server: mcpServer({ id: 4, name: 'github' }), status: 'stopped' }),
      mcpServerInfo({ server: mcpServer({ id: 5, name: 'brave' }), status: 'starting' }),
      mcpServerInfo({ server: mcpServer({ id: 6, name: 'broken' }), status: { error: 'spawn npx ENOENT' } }),
      sse(),
    );

    expect(row('tavily').getByText('Running')).toBeInTheDocument();
    expect(row('tavily').getByText('2 tools')).toBeInTheDocument();
    expect(row('github').getByText('Stopped')).toBeInTheDocument();
    expect(row('github').queryByText(/tools?$/)).not.toBeInTheDocument();
    expect(row('brave').getByText('Starting')).toBeInTheDocument();
    expect(row('broken').getByText('Failed')).toBeInTheDocument();
    expect(row('broken').getByText('spawn npx ENOENT')).toBeInTheDocument();
    expect(row('remote').getByText('Not supported yet')).toBeInTheDocument();
  });

  it('offers a stopped server Test, Start, Edit and Remove…, and a running one Stop', async () => {
    await openTools(tavily(), mcpServerInfo({ server: mcpServer({ id: 4, name: 'github' }), status: 'running' }));

    for (const name of ['Test', 'Start', 'Edit', 'More for tavily']) {
      expect(row('tavily').getByRole('button', { name })).toBeEnabled();
    }
    expect(row('tavily').queryByRole('button', { name: 'Stop' })).not.toBeInTheDocument();
    expect(row('github').getByRole('button', { name: 'Stop' })).toBeEnabled();
    expect(row('github').queryByRole('button', { name: 'Start' })).not.toBeInTheDocument();
  });

  it('offers a server gglib cannot run only Remove…, and removes it', async () => {
    const user = await openTools(sse());

    for (const name of ['Test', 'Start', 'Stop', 'Edit', 'Fix']) {
      expect(row('remote').queryByRole('button', { name })).not.toBeInTheDocument();
    }
    await user.click(await removeItem(user, 'remote'));
    await user.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Remove' }));
    await waitFor(() => expect(transport.removeMcpServer).toHaveBeenCalledWith(7));
  });

  it('greys Start for a server not used in chats, and Remove… while it runs, each with why', async () => {
    const user = await openTools(
      tavily({ enabled: false }),
      mcpServerInfo({ server: mcpServer({ id: 4, name: 'github' }), status: 'running' }),
    );

    const start = row('tavily').getByRole('button', { name: 'Start' });
    expect(start).toBeDisabled();
    expect(start).toHaveAttribute('title', 'Not used in chats: Edit it and tick Use in chats to start it');

    const remove = await removeItem(user, 'github');
    expect(remove).toBeDisabled();
    expect(remove).toHaveAttribute('title', 'Stop it before removing it');
  });

  it('starts and stops a server, the row following the daemon', async () => {
    daemon.tools = tools;
    const user = await openTools(tavily());

    await user.click(row('tavily').getByRole('button', { name: 'Start' }));
    expect(transport.startMcpServer).toHaveBeenCalledWith(3);
    expect(await row('tavily').findByText('Running')).toBeInTheDocument();
    expect(row('tavily').getByText('2 tools')).toBeInTheDocument();

    await user.click(row('tavily').getByRole('button', { name: 'Stop' }));
    expect(transport.stopMcpServer).toHaveBeenCalledWith(3);
    expect(await row('tavily').findByText('Stopped')).toBeInTheDocument();
  });

  it('says a refused start under its row, in the daemon\'s words', async () => {
    const user = await openTools(tavily(), mcpServerInfo({ server: mcpServer({ id: 4, name: 'github' }) }));
    daemon.refuse = 'npx was not found on PATH';

    await user.click(row('tavily').getByRole('button', { name: 'Start' }));

    expect(await row('tavily').findByText('npx was not found on PATH')).toBeInTheDocument();
    expect(row('github').queryByText('npx was not found on PATH')).not.toBeInTheDocument();
  });
});

describe('Test', () => {
  it('shows what the server offers under its row alone', async () => {
    daemon.tested = { ok: true, error: null, tools };
    const user = await openTools(tavily(), mcpServerInfo({ server: mcpServer({ id: 4, name: 'github' }) }));

    await user.click(row('tavily').getByRole('button', { name: 'Test' }));

    expect(await row('tavily').findByText('Connected — 2 tools offered')).toBeInTheDocument();
    expect(row('tavily').getByTitle('Search the web')).toHaveTextContent('search');
    expect(row('tavily').getByText('extract')).toBeInTheDocument();
    expect(row('github').queryByText(/Connected/)).not.toBeInTheDocument();
    expect(transport.testMcpServer).toHaveBeenCalledWith(3);
  });

  it('shows why a connection failed', async () => {
    daemon.tested = { ok: false, error: 'exited with status 1', tools: [] };
    const user = await openTools(tavily());

    await user.click(row('tavily').getByRole('button', { name: 'Test' }));

    expect(await row('tavily').findByText('Connection failed — exited with status 1')).toBeInTheDocument();
  });

  it('runs one test at a time, saying why another row\'s Test is greyed', async () => {
    daemon.tested = null;
    const user = await openTools(tavily(), mcpServerInfo({ server: mcpServer({ id: 4, name: 'github' }) }));

    await user.click(row('tavily').getByRole('button', { name: 'Test' }));

    const other = row('github').getByRole('button', { name: 'Test' });
    await waitFor(() => expect(other).toBeDisabled());
    expect(other).toHaveAttribute('title', 'Another test is running');
    answerTest({ ok: true, error: null, tools: [] });
    await waitFor(() => expect(other).toBeEnabled());
  });
});

describe('Needs relink', () => {
  it('offers Fix on a stdio server whose command no longer resolves, and drops it once fixed', async () => {
    const user = await openTools(tavily({ is_valid: false, last_error: 'npx: not found' }), mcpServerInfo({ server: mcpServer({ id: 4, name: 'github' }) }));

    expect(row('tavily').getByText('Needs relink')).toBeInTheDocument();
    expect(row('tavily').getByText('npx: not found')).toBeInTheDocument();
    expect(row('github').queryByText('Needs relink')).not.toBeInTheDocument();

    await user.click(row('tavily').getByRole('button', { name: 'Fix' }));

    expect(transport.resolveMcpServerPath).toHaveBeenCalledWith(3);
    expect(await row('tavily').findByText('Resolved: /usr/local/bin/npx')).toBeInTheDocument();
    await waitFor(() => expect(row('tavily').queryByText('Needs relink')).not.toBeInTheDocument());
  });

  it('says what to do when the command cannot be found', async () => {
    daemon.resolution = {
      success: false,
      attempts: [{ candidate: '/usr/bin/npx', outcome: 'not found' }],
      warnings: [],
      suggested_fix: 'brew install node',
    };
    const user = await openTools(tavily({ is_valid: false }));

    await user.click(row('tavily').getByRole('button', { name: 'Fix' }));

    expect(
      await row('tavily').findByText(
        'Could not resolve executable. Suggested fix: brew install node Install Node.js if not installed (includes npm/npx).',
      ),
    ).toBeInTheDocument();
    expect(row('tavily').getByText('Needs relink')).toBeInTheDocument();
  });
});

describe('an opened row', () => {
  it('shows the command, the tools and Devices may use it only once opened', async () => {
    const user = await openTools(tavily({}, { status: 'running', tools }));
    const name = opener('tavily');
    expect(name).toHaveAttribute('aria-expanded', 'false');
    expect(row('tavily').queryByText('npx -y tavily-mcp')).not.toBeInTheDocument();

    await user.click(name);

    expect(name).toHaveAttribute('aria-expanded', 'true');
    expect(row('tavily').getByText('npx -y tavily-mcp')).toBeInTheDocument();
    expect(row('tavily').getByText('search')).toBeInTheDocument();
    expect(row('tavily').getByRole('checkbox', { name: 'Devices may use it' })).not.toBeChecked();
  });

  it('opens and closes the server to paired devices, and says a refusal beside the box', async () => {
    const user = await openTools(tavily());
    await user.click(opener('tavily'));
    const box = () => row('tavily').getByRole('checkbox', { name: 'Devices may use it' });

    await user.click(box());
    expect(transport.setMcpServerRemote).toHaveBeenCalledWith(3, true);
    await waitFor(() => expect(box()).toBeChecked());

    await user.click(box());
    expect(transport.setMcpServerRemote).toHaveBeenLastCalledWith(3, false);
    await waitFor(() => expect(box()).not.toBeChecked());

    daemon.refuse = 'the daemon said no';
    await user.click(box());
    expect(await row('tavily').findByText('the daemon said no')).toBeInTheDocument();
    expect(box()).not.toBeChecked();
  });

  it('never offers an SSE server or the builtin to devices, and says a disabled one offers nothing', async () => {
    const user = await openTools(
      tavily({ enabled: false, allow_remote: true }),
      sse(),
      mcpServerInfo({ server: mcpServer({ id: 9, name: 'builtin' }) }),
    );
    for (const name of ['tavily', 'remote', 'builtin']) await user.click(opener(name));

    expect(row('tavily').getByRole('checkbox', { name: /Devices may use it/ })).toBeChecked();
    expect(row('tavily').getByText('Disabled, so it offers no tools')).toBeInTheDocument();
    expect(row('remote').queryByRole('checkbox')).not.toBeInTheDocument();
    expect(row('remote').getByText('http://localhost:3001/sse')).toBeInTheDocument();
    expect(row('builtin').queryByRole('checkbox')).not.toBeInTheDocument();
  });
});

describe('the add sheet', () => {
  it('sends every field under its wire name', async () => {
    const user = await openTools();
    await user.click(screen.getByRole('button', { name: 'Add a server' }));
    expect(screen.getByRole('dialog', { name: 'Add a server' })).toBeInTheDocument();

    await user.type(sheet().getByLabelText('Name'), 'files');
    await user.type(sheet().getByLabelText('Command'), 'uvx');
    await user.type(sheet().getByLabelText('Arguments'), 'mcp-files  --ro');
    await user.click(sheet().getByRole('button', { name: /add variable/i }));
    await user.type(sheet().getByLabelText('Variable name'), 'TOKEN');
    await user.type(sheet().getByLabelText('TOKEN value'), 's3cret');
    await user.click(sheet().getByRole('checkbox', { name: /^Use in chats/ }));
    await user.click(sheet().getByRole('button', { name: 'More' }));
    await user.type(sheet().getByLabelText('Working directory'), '/srv/files');
    await user.type(sheet().getByLabelText('Extra PATH'), '/opt/uv/bin');
    await user.click(sheet().getByRole('checkbox', { name: /^Start when gglib starts/ }));
    await user.click(sheet().getByRole('button', { name: 'Add' }));

    await waitFor(() => expect(transport.addMcpServer).toHaveBeenCalledTimes(1));
    expect(transport.addMcpServer).toHaveBeenCalledWith({
      name: 'files',
      server_type: 'stdio',
      enabled: false,
      lifecycle: 'eager',
      env: [{ key: 'TOKEN', value: 's3cret' }],
      config: { command: 'uvx', args: ['mcp-files', '--ro'], working_dir: '/srv/files', path_extra: '/opt/uv/bin' },
    });
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  });

  it('is used in chats and started on first use unless told otherwise', async () => {
    const user = await openTools();
    await user.click(screen.getByRole('button', { name: 'Add a server' }));
    await user.type(sheet().getByLabelText('Name'), 'echo');
    await user.type(sheet().getByLabelText('Command'), 'cat');
    await user.click(sheet().getByRole('button', { name: 'Add' }));

    await waitFor(() => expect(transport.addMcpServer).toHaveBeenCalledTimes(1));
    expect(transport.addMcpServer).toHaveBeenCalledWith({
      name: 'echo',
      server_type: 'stdio',
      enabled: true,
      lifecycle: 'lazy',
      env: [],
      config: { command: 'cat', args: [], working_dir: undefined, path_extra: undefined },
    });
  });

  it('keeps Working directory, Extra PATH and Start when gglib starts under a closed More', async () => {
    const user = await openTools();
    await user.click(screen.getByRole('button', { name: 'Add a server' }));

    expect(sheet().getByRole('button', { name: 'More' })).toHaveAttribute('aria-expanded', 'false');
    for (const label of ['Working directory', 'Extra PATH']) expect(sheet().queryByLabelText(label)).not.toBeInTheDocument();
    expect(sheet().queryByRole('checkbox', { name: /^Start when gglib starts/ })).not.toBeInTheDocument();
  });

  it('fills the fields from a quick start, its key waiting for a value', async () => {
    const user = await openTools();
    await user.click(screen.getByRole('button', { name: 'Add a server' }));

    await user.click(sheet().getByRole('button', { name: /^GitHub/ }));

    expect(sheet().getByLabelText('Name')).toHaveValue('GitHub');
    expect(sheet().getByLabelText('Command')).toHaveValue('npx');
    expect(sheet().getByLabelText('Arguments')).toHaveValue('-y @modelcontextprotocol/server-github');
    expect(sheet().getByLabelText('Variable name')).toHaveValue('GITHUB_PERSONAL_ACCESS_TOKEN');
    expect(sheet().getByLabelText('GITHUB_PERSONAL_ACCESS_TOKEN value')).toHaveValue('');
  });

  it('offers no filesystem quick start, says reading files is built in, and every one names its key', async () => {
    const user = await openTools();
    await user.click(screen.getByRole('button', { name: 'Add a server' }));

    expect(sheet().queryByText(/Filesystem/)).not.toBeInTheDocument();
    for (const name of ['Tavily Web Search', 'GitHub', 'Brave Search']) expect(sheet().getByText(name)).toBeInTheDocument();
    expect(
      sheet().getByText('Reading files is built in: choose a folder in a chat. Every template here needs an API key.'),
    ).toBeInTheDocument();
    for (const template of SERVER_TEMPLATES) expect(template.envKeys.length, template.name).toBeGreaterThan(0);
  });

  it.each([
    ['adding', undefined],
    ['editing', 'tavily'],
  ])('offers no connection type and no URL while %s', async (_, edit) => {
    const user = await openTools(tavily());
    await user.click(edit ? row(edit).getByRole('button', { name: 'Edit' }) : screen.getByRole('button', { name: 'Add a server' }));
    if (!edit) await user.click(sheet().getByRole('button', { name: 'More' }));
    else expect(sheet().getByLabelText('Name')).toHaveValue('tavily');

    expect(sheet().queryByRole('radio')).not.toBeInTheDocument();
    expect(sheet().queryByText(/SSE/)).not.toBeInTheDocument();
    expect(sheet().queryByText(/URL/)).not.toBeInTheDocument();
  });

  it('types a new variable into a masked input with an eye', async () => {
    const user = await openTools();
    await user.click(screen.getByRole('button', { name: 'Add a server' }));
    await user.click(sheet().getByRole('button', { name: /add variable/i }));

    expect(sheet().getByLabelText('New variable value')).toHaveAttribute('type', 'password');
    expect(sheet().getByRole('button', { name: 'Show the typed New variable value' })).toBeInTheDocument();
  });

  it('says what is missing, and a refused add in the daemon\'s words with what was typed kept', async () => {
    const user = await openTools();
    await user.click(screen.getByRole('button', { name: 'Add a server' }));
    await user.click(sheet().getByRole('button', { name: 'Add' }));
    expect(sheet().getByRole('alert')).toHaveTextContent('A server needs a name');

    await user.type(sheet().getByLabelText('Name'), 'files');
    await user.click(sheet().getByRole('button', { name: 'Add' }));
    expect(sheet().getByRole('alert')).toHaveTextContent('A server needs a command');

    await user.type(sheet().getByLabelText('Command'), 'npx -y files');
    await user.click(sheet().getByRole('button', { name: 'Add' }));
    expect(sheet().getByRole('alert')).toHaveTextContent('Command is one executable, such as npx; its arguments go in Arguments');
    expect(transport.addMcpServer).not.toHaveBeenCalled();

    daemon.refuse = 'a server named files already exists';
    await user.clear(sheet().getByLabelText('Command'));
    await user.type(sheet().getByLabelText('Command'), 'npx');
    await user.click(sheet().getByRole('button', { name: 'Add' }));
    expect(await sheet().findByText('a server named files already exists')).toBeInTheDocument();
    expect(sheet().getByLabelText('Name')).toHaveValue('files');
  });
});

describe('Edit', () => {
  const stored = () =>
    tavily({ env_keys: ['TOKEN', 'ROOT'], lifecycle: 'eager', config: { command: 'npx', args: ['-y', 'tavily-mcp'], working_dir: '/srv' } });

  it('opens the same sheet filled from the stored server, More open where it holds something', async () => {
    const user = await openTools(stored());
    await user.click(row('tavily').getByRole('button', { name: 'Edit' }));

    expect(screen.getByRole('dialog', { name: 'Edit tavily' })).toBeInTheDocument();
    expect(sheet().queryByText('Tavily Web Search')).not.toBeInTheDocument();
    expect(sheet().getByLabelText('Name')).toHaveValue('tavily');
    expect(sheet().getByLabelText('Command')).toHaveValue('npx');
    expect(sheet().getByLabelText('Arguments')).toHaveValue('-y tavily-mcp');
    expect(sheet().getByRole('checkbox', { name: /^Use in chats/ })).toBeChecked();
    expect(sheet().getByRole('button', { name: 'More' })).toHaveAttribute('aria-expanded', 'true');
    expect(sheet().getByLabelText('Working directory')).toHaveValue('/srv');
    expect(sheet().getByRole('checkbox', { name: /^Start when gglib starts/ })).toBeChecked();
  });

  it('shows a stored variable by its key, read-only, its value held, and sends it back without one', async () => {
    const user = await openTools(stored());
    await user.click(row('tavily').getByRole('button', { name: 'Edit' }));

    const key = sheet().getByLabelText('TOKEN, kept');
    expect(key).toHaveValue('TOKEN');
    expect(key).toHaveAttribute('readonly');
    expect(sheet().getByLabelText('TOKEN value')).toHaveValue(MASK);

    await user.click(sheet().getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(transport.updateMcpServer).toHaveBeenCalledTimes(1));
    expect(transport.updateMcpServer).toHaveBeenCalledWith(3, {
      name: 'tavily',
      server_type: 'stdio',
      enabled: true,
      lifecycle: 'eager',
      env: [{ key: 'TOKEN' }, { key: 'ROOT' }],
      config: { command: 'npx', args: ['-y', 'tavily-mcp'], working_dir: '/srv', path_extra: undefined },
    });
  });

  it('sends a replaced value, and leaves a removed variable out', async () => {
    const user = await openTools(stored());
    await user.click(row('tavily').getByRole('button', { name: 'Edit' }));
    await user.click(sheet().getAllByRole('button', { name: 'Replace' })[0]);
    await user.type(sheet().getByLabelText('TOKEN value'), 'rotated');
    await user.click(sheet().getByRole('button', { name: 'Remove ROOT' }));
    await user.click(sheet().getByRole('button', { name: 'Save' }));

    await waitFor(() => expect(transport.updateMcpServer).toHaveBeenCalledTimes(1));
    expect(transport.updateMcpServer.mock.calls[0][1].env).toEqual([{ key: 'TOKEN', value: 'rotated' }]);
  });

  it('shows the next variable masked after a revealed one is removed', async () => {
    transport.revealSecret.mockResolvedValueOnce('t0k-81d0e4');
    const user = await openTools(stored());
    await user.click(row('tavily').getByRole('button', { name: 'Edit' }));
    await user.click(sheet().getByRole('button', { name: 'Show the TOKEN value' }));
    await waitFor(() => expect(sheet().getByLabelText('TOKEN value')).toHaveValue('t0k-81d0e4'));

    await user.click(sheet().getByRole('button', { name: 'Remove TOKEN' }));

    expect(sheet().getByLabelText('ROOT value')).toHaveValue(MASK);
    expect(sheet().queryByDisplayValue('t0k-81d0e4')).not.toBeInTheDocument();
  });
});

describe('Remove…', () => {
  it('asks first, sends nothing on Cancel, and takes the row away on Remove', async () => {
    const user = await openTools(tavily());

    await user.click(await removeItem(user, 'tavily'));
    const ask = within(await screen.findByRole('dialog'));
    expect(ask.getByText('Remove "tavily"?')).toBeInTheDocument();
    await user.click(ask.getByRole('button', { name: 'Cancel' }));
    expect(transport.removeMcpServer).not.toHaveBeenCalled();

    await user.click(await removeItem(user, 'tavily'));
    await user.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Remove' }));
    await waitFor(() => expect(transport.removeMcpServer).toHaveBeenCalledWith(3));
    await waitFor(() => expect(screen.queryByRole('listitem', { name: 'tavily' })).not.toBeInTheDocument());
  });
});

describe('one list (ST-8)', () => {
  it('is read once on opening, lists one added through the sheet, and offers no Refresh', async () => {
    const user = await openTools(mcpServerInfo({ server: mcpServer({ id: 7, name: 'from-the-cli' }) }));
    expect(transport.listMcpServers).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('button', { name: 'Refresh' })).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Add a server' }));
    await user.type(sheet().getByLabelText('Name'), 'files');
    await user.type(sheet().getByLabelText('Command'), 'cat');
    await user.click(sheet().getByRole('button', { name: 'Add' }));

    expect(await screen.findByRole('listitem', { name: 'files' })).toBeInTheDocument();
    expect(screen.getByRole('listitem', { name: 'from-the-cli' })).toBeInTheDocument();
  });

  it('edits through the list the rows show, so a second edit starts from the first', async () => {
    const user = await openTools(tavily({ env_keys: ['TOKEN'] }));
    await user.click(row('tavily').getByRole('button', { name: 'Edit' }));
    await user.click(sheet().getByRole('button', { name: 'Remove TOKEN' }));
    await user.click(sheet().getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());

    await user.click(row('tavily').getByRole('button', { name: 'Edit' }));

    expect(sheet().queryByLabelText('TOKEN, kept')).not.toBeInTheDocument();
  });

  it('says there are none in one sentence', async () => {
    await openTools();
    expect(screen.getByText('No tool servers yet. Add one to give chats tools such as web search.')).toBeInTheDocument();
    expect(screen.queryByRole('list')).not.toBeInTheDocument();
  });
});

describe('the rest of Tools', () => {
  it('holds the agent limits, this device\'s among them, under one disclosure', async () => {
    const user = await openTools();
    const limits = screen.getByRole('button', { name: 'Agent limits' });
    expect(screen.queryByLabelText('Max tool rounds')).not.toBeInTheDocument();
    await user.click(limits);
    expect(screen.getByLabelText('Max tool rounds')).toBeInTheDocument();
    expect(screen.getByLabelText('Max stuck steps')).toBeInTheDocument();
    expect(within(screen.getByRole('region', { name: 'This device' })).getByText('applies to all chats on this device')).toBeInTheDocument();
  });

  it('puts the tool servers first and the footnote last', async () => {
    await openTools();
    const servers = screen.getByRole('region', { name: 'Tool servers' });
    const draw = screen.getByRole('checkbox', { name: /^Let MCP clients draw/ });
    const footnote = screen.getByRole('link', { name: 'Learn more about MCP' });

    expect(servers.compareDocumentPosition(draw) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(draw.compareDocumentPosition(screen.getByLabelText('Title prompt')) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(footnote).toHaveAttribute('href', 'https://modelcontextprotocol.io/introduction');
    expect(screen.getByLabelText('Title prompt').compareDocumentPosition(footnote) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});
