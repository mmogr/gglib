/**
 * The settings place, at `/settings/:section` through the real routes: each
 * section by its address and by its list, `/settings` and a section that is
 * none going to General, the same section after a reload; a field saved the
 * moment it changes as a write of that field alone, Undo for five seconds
 * writing back what was stored, nothing written for a value already stored;
 * a refused write said under its field with what was typed kept; the API
 * key's Show, Copy, Replace (asked first), Clear and Undo; the restart
 * notes; and the interim Tools, Engines and About hosting today's panels.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, screen, waitFor, within } from '@testing-library/react';
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
vi.mock('../../../src/services/transport/api/proxy', () => ({ getLoopGuardTrips: vi.fn(async () => []) }));
vi.mock('../../../src/places/settings/EngineCard', () => ({ EngineCard: ({ kind }: { kind: string }) => <p>engine card: {kind}</p> }));
vi.mock('../../../src/places/settings/DiagnosticsPanel', () => ({ DiagnosticsPanel: () => <p>system checks</p> }));
vi.mock('../../../src/places/library/LibraryPlace', () => ({ default: () => <main aria-label="Library" /> }));

import { daemon, holdWrites, resetDaemon, transport, writes } from './settingsDaemon';
import { renderApp } from './harness';
import { mcpServer, mcpServerInfo } from '../fixtures/mcp';

const SECTIONS = ['General', 'Endpoint', 'Models', 'Tools', 'Engines', 'About'];
const heading = () => screen.getByRole('heading', { level: 2 });
const list = () => within(screen.getByRole('navigation', { name: 'Settings sections' }));
const box = (label: string) => screen.getByLabelText(label) as HTMLInputElement;
/** A checkbox, by the label its description follows. */
const check = (label: string) => screen.getByRole('checkbox', { name: new RegExp(`^${label}`) });

beforeEach(() => resetDaemon());
afterEach(() => vi.useRealTimers());

describe('the address', () => {
  it.each([
    ['general', 'General', 'Start the endpoint when gglib launches'],
    ['endpoint', 'Endpoint', 'Port'],
    ['models', 'Models', 'Download queue size'],
    ['tools', 'Tools', 'Let MCP clients draw'],
    ['engines', 'Engines', 'engine card: llama'],
    ['about', 'About', 'Stop the daemon'],
  ])('/settings/%s opens %s', async (section, name, inside) => {
    renderApp(`/settings/${section}`);
    expect(heading()).toHaveTextContent(name);
    expect(await screen.findAllByText(inside)).not.toHaveLength(0);
    expect(list().getByRole('button', { name })).toHaveAttribute('aria-current', 'page');
  });

  it('lists the six sections in order, and a pick moves the address', async () => {
    const app = renderApp('/settings/general');
    expect(list().getAllByRole('button').map((b) => b.textContent)).toEqual(SECTIONS);
    await userEvent.setup().click(list().getByRole('button', { name: 'Models' }));
    expect(app.at()).toBe('/settings/models');
    expect(heading()).toHaveTextContent('Models');
    expect(list().getByRole('button', { name: 'Models' })).toHaveAttribute('aria-current', 'page');
    expect(list().getByRole('button', { name: 'General' })).not.toHaveAttribute('aria-current');
  });

  it('goes to General from /settings and from a section that is none', async () => {
    const bare = renderApp('/settings');
    await waitFor(() => expect(bare.at()).toBe('/settings/general'));
    bare.unmount();
    const wrong = renderApp('/settings/system');
    await waitFor(() => expect(wrong.at()).toBe('/settings/general'));
  });

  it('opens the same section on a reload, and reads the settings again', async () => {
    renderApp('/settings/endpoint').unmount();
    const reads = transport.getSettings.mock.calls.length;
    renderApp('/settings/endpoint');
    expect(heading()).toHaveTextContent('Endpoint');
    await waitFor(() => expect(transport.getSettings.mock.calls.length).toBeGreaterThan(reads));
  });
});

describe('saving as you go', () => {
  it('writes a ticked box alone, the moment it moves, and Undo writes back what was stored', async () => {
    renderApp('/settings/general');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('checkbox', { name: /^Start the endpoint when gglib launches/ }));
    await waitFor(() => expect(writes()).toEqual([{ proxyAutostart: true }]));
    expect(check('Start the endpoint when gglib launches')).toBeChecked();

    await user.click(await screen.findByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(writes()).toEqual([{ proxyAutostart: true }, { proxyAutostart: null }]));
    expect(check('Start the endpoint when gglib launches')).not.toBeChecked();
    expect(screen.queryByRole('button', { name: 'Undo' })).not.toBeInTheDocument();
  });

  it('writes a box when it is left, and offers Undo for five seconds', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    resetDaemon({ proxyPort: 8081 });
    renderApp('/settings/endpoint');
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    const port = await screen.findByLabelText('Port');
    await user.clear(port);
    await user.type(port, '8082');
    expect(writes()).toEqual([]);
    await user.tab();
    await waitFor(() => expect(writes()).toEqual([{ proxyPort: 8082 }]));
    expect(await screen.findByRole('button', { name: 'Undo' })).toBeInTheDocument();

    await act(async () => {
      vi.advanceTimersByTime(4_900);
    });
    expect(screen.getByRole('button', { name: 'Undo' })).toBeInTheDocument();
    await act(async () => {
      vi.advanceTimersByTime(200);
    });
    expect(screen.queryByRole('button', { name: 'Undo' })).not.toBeInTheDocument();
  });

  it('writes on Enter, puts back the stored value on Escape, and writes nothing for the value stored', async () => {
    resetDaemon({ proxyPort: 8081 });
    renderApp('/settings/endpoint');
    const user = userEvent.setup();
    const port = await screen.findByLabelText('Port');
    await user.clear(port);
    await user.type(port, '9999{Escape}');
    expect(port).toHaveValue(8081);
    await user.clear(port);
    await user.type(port, '8081{Enter}');
    await user.tab();
    expect(writes()).toEqual([]);
    expect(screen.queryByRole('button', { name: 'Undo' })).not.toBeInTheDocument();

    await user.clear(port);
    await user.type(port, '8090{Enter}');
    await waitFor(() => expect(writes()).toEqual([{ proxyPort: 8090 }]));
  });

  it('says a refused write under its field, in the daemon\'s words, and keeps what was typed', async () => {
    resetDaemon({ proxyPort: 8081 });
    daemon.refuse = 'proxy_port must be at least 1024';
    renderApp('/settings/endpoint');
    const user = userEvent.setup();
    const port = await screen.findByLabelText('Port');
    await user.clear(port);
    await user.type(port, '80{Enter}');

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('proxy_port must be at least 1024');
    expect(port).toHaveValue(80);
    expect(port).toHaveAttribute('aria-invalid', 'true');
    expect(screen.queryByRole('button', { name: 'Undo' })).not.toBeInTheDocument();

    daemon.refuse = null;
    await user.clear(port);
    await user.type(port, '8085{Enter}');
    await waitFor(() => expect(screen.queryByRole('alert')).not.toBeInTheDocument());
    expect(daemon.settings.proxyPort).toBe(8085);
  });

  it('draws no Save and no Cancel', async () => {
    renderApp('/settings/models');
    await screen.findByLabelText('Download queue size');
    expect(screen.queryByRole('button', { name: /^save/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^cancel/i })).not.toBeInTheDocument();
  });
});

describe('a write on its way', () => {
  it('holds the box until it lands, so an edit then is not compared with the value before it', async () => {
    resetDaemon({ proxyPort: 8081 });
    const release = holdWrites();
    renderApp('/settings/endpoint');
    const user = userEvent.setup();
    const port = await screen.findByLabelText('Port');
    await user.clear(port);
    await user.type(port, '8082{Enter}');
    await waitFor(() => expect(port).toBeDisabled());
    await user.tab();
    expect(writes()).toEqual([{ proxyPort: 8082 }]);

    release();
    await waitFor(() => expect(port).toBeEnabled());
    await user.clear(port);
    await user.type(port, '8081{Enter}');
    await waitFor(() => expect(writes()).toEqual([{ proxyPort: 8082 }, { proxyPort: 8081 }]));
    expect(daemon.settings.proxyPort).toBe(8081);
  });

  it('holds a checkbox until it lands', async () => {
    const release = holdWrites();
    renderApp('/settings/general');
    const user = userEvent.setup();
    const box = await screen.findByRole('checkbox', { name: /^Start the endpoint when gglib launches/ });
    await user.click(box);
    await waitFor(() => expect(box).toBeDisabled());
    release();
    await waitFor(() => expect(box).toBeEnabled());
    expect(box).toBeChecked();
  });

  it('keeps Undo for five seconds after the latest of two writes, not the first', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    resetDaemon({ proxyPort: 8081 });
    renderApp('/settings/endpoint');
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    const port = await screen.findByLabelText('Port');
    await user.clear(port);
    await user.type(port, '8082{Enter}');
    await screen.findByRole('button', { name: 'Undo' });
    await act(async () => {
      vi.advanceTimersByTime(3_000);
    });
    await user.clear(port);
    await user.type(port, '8083{Enter}');
    await waitFor(() => expect(writes()).toEqual([{ proxyPort: 8082 }, { proxyPort: 8083 }]));
    await act(async () => {
      vi.advanceTimersByTime(2_500);
    });
    expect(screen.getByRole('button', { name: 'Undo' })).toBeInTheDocument();
    await act(async () => {
      vi.advanceTimersByTime(3_000);
    });
    expect(screen.queryByRole('button', { name: 'Undo' })).not.toBeInTheDocument();
  });
});

describe('opening the place', () => {
  it('reads the settings again, though the provider is already up', async () => {
    const app = renderApp('/library');
    await waitFor(() => expect(transport.getSettings).toHaveBeenCalledTimes(1));
    await act(() => app.router.navigate('/settings/general'));
    await waitFor(() => expect(transport.getSettings).toHaveBeenCalledTimes(2));
  });
});

describe('the API key', () => {
  const stubClipboard = () => {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    return writeText;
  };

  it('sets a key where none is stored, as the key alone, and Undo clears it', async () => {
    renderApp('/settings/endpoint');
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText('API key'), 'sk-first');
    await user.click(screen.getByRole('button', { name: 'Set' }));
    await waitFor(() => expect(writes()).toEqual([{ proxyApiKey: 'sk-first' }]));
    await user.click(await screen.findByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(writes()).toEqual([{ proxyApiKey: 'sk-first' }, { proxyApiKey: null }]));
  });

  it('shows and copies a held key only when asked', async () => {
    resetDaemon({}, 'sk-held');
    renderApp('/settings/endpoint');
    const user = userEvent.setup();
    // After the setup, which puts a clipboard of its own on `navigator`.
    const writeText = stubClipboard();
    expect(await screen.findByLabelText('API key')).toHaveValue('••••••••');
    expect(transport.revealSecret).not.toHaveBeenCalled();
    await user.click(screen.getByRole('button', { name: 'Show the API key' }));
    await waitFor(() => expect(box('API key')).toHaveValue('sk-held'));
    await user.click(screen.getByRole('button', { name: 'Hide the API key' }));
    expect(box('API key')).toHaveValue('••••••••');
    await user.click(screen.getByRole('button', { name: 'Copy the API key' }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('sk-held'));
    expect(writes()).toEqual([]);
  });

  it('asks before replacing a key, sends nothing on Cancel, and Undo writes the old key back', async () => {
    resetDaemon({}, 'sk-old');
    renderApp('/settings/endpoint');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Replace' }));
    await user.type(screen.getByLabelText('API key'), 'sk-new');
    await user.click(screen.getByRole('button', { name: 'Replace' }));
    expect(await screen.findByText('Replace the API key?')).toBeInTheDocument();
    expect(screen.getByText('Clients that send the old key are refused within a few seconds.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(writes()).toEqual([]);

    await user.click(screen.getByRole('button', { name: 'Replace' }));
    const ask = await screen.findByRole('dialog');
    await user.click(within(ask).getByRole('button', { name: 'Replace' }));
    await waitFor(() => expect(writes()).toEqual([{ proxyApiKey: 'sk-new' }]));
    expect(box('API key')).toHaveValue('••••••••');

    await user.click(await screen.findByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(writes()).toEqual([{ proxyApiKey: 'sk-new' }, { proxyApiKey: 'sk-old' }]));
    expect(daemon.key).toBe('sk-old');
  });

  it('clears a key at once, and Undo writes it back', async () => {
    resetDaemon({}, 'sk-old');
    renderApp('/settings/endpoint');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Clear' }));
    await waitFor(() => expect(writes()).toEqual([{ proxyApiKey: null }]));
    expect(await screen.findByRole('button', { name: 'Set' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(daemon.key).toBe('sk-old'));
    expect(writes()).toEqual([{ proxyApiKey: null }, { proxyApiKey: 'sk-old' }]);
  });

  it('keeps the typed key when the Replace is refused, with the refusal under it', async () => {
    resetDaemon({}, 'sk-old');
    daemon.refuse = 'the settings could not be saved';
    renderApp('/settings/endpoint');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Replace' }));
    await user.type(screen.getByLabelText('API key'), 'sk-new');
    await user.click(screen.getByRole('button', { name: 'Replace' }));
    await user.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Replace' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('the settings could not be saved');
    expect(box('API key')).toHaveValue('sk-new');
    expect(daemon.key).toBe('sk-old');
  });

  it('writes nothing when the key in use cannot be read for Undo', async () => {
    resetDaemon({}, 'sk-old');
    transport.revealSecret.mockRejectedValueOnce(new Error('the daemon is locked'));
    renderApp('/settings/endpoint');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Clear' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('could not be read, so it was left as it is: the daemon is locked');
    expect(writes()).toEqual([]);
  });
});

describe('the interim sections', () => {
  it('Tools hosts the tool servers, read when it opens, and lists one added without a Refresh (ST-8)', async () => {
    daemon.servers.push(mcpServerInfo({ server: mcpServer({ id: 7, name: 'from-the-cli' }) }));
    renderApp('/settings/tools');
    const user = userEvent.setup();
    expect(await screen.findByText('from-the-cli')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /add server/i }));
    await user.type(screen.getByPlaceholderText('My MCP Server'), 'files');
    await user.type(screen.getByPlaceholderText('npx, python3, node'), 'cat');
    await user.click(screen.getByRole('button', { name: /^add server$/i }));
    expect(await screen.findByText('files')).toBeInTheDocument();
  });

  it('Tools holds the agent limits, this device\'s among them, under one disclosure', async () => {
    renderApp('/settings/tools');
    const user = userEvent.setup();
    const limits = await screen.findByRole('button', { name: 'Agent limits' });
    expect(screen.queryByLabelText('Max tool rounds')).not.toBeInTheDocument();
    await user.click(limits);
    expect(screen.getByLabelText('Max tool rounds')).toBeInTheDocument();
    expect(screen.getByLabelText('Max stuck steps')).toBeInTheDocument();
    expect(within(screen.getByRole('region', { name: 'This device' })).getByText('applies to all chats on this device')).toBeInTheDocument();
  });

  it('Engines hosts an engine card for each engine and the system checks', () => {
    renderApp('/settings/engines');
    expect(screen.getByText('engine card: llama')).toBeInTheDocument();
    expect(screen.getByText('engine card: stable_diffusion')).toBeInTheDocument();
    expect(screen.getByText('system checks')).toBeInTheDocument();
  });

  it('About shows the build and stops the daemon only when asked', async () => {
    renderApp('/settings/about');
    const user = userEvent.setup();
    expect(await screen.findByText('0.31.0 (abc1234)')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Stop the daemon' }));
    await user.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Cancel' }));
    expect(transport.shutdownDaemon).not.toHaveBeenCalled();
    await user.click(screen.getByRole('button', { name: 'Stop the daemon' }));
    await user.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Stop' }));
    await waitFor(() => expect(transport.shutdownDaemon).toHaveBeenCalledTimes(1));
    expect(await screen.findByText('Shutdown requested. This interface will stop responding.')).toBeInTheDocument();
  });
});
