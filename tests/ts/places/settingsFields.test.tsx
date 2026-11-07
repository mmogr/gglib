/**
 * The settings place's fields, each through the real routes: every field
 * the place rebuilt writes its binding's own name and no other field (ST-1,
 * ST-7), with the value it means; a number's default in the box and under
 * it, announced with the field (ST-2), and the range the backend accepts
 * (ST-3); the models folder's Reset writing the folder alone (ST-4); the
 * loop guard as on and off with the terminal's Refuse shown; the image model
 * choice keeping a stored id that is no image model; the sampling summary
 * and Change…; a profile added from the place; the menu bar toggle in the
 * desktop app only.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

const desktop = vi.hoisted(() => ({ is: false, picked: null as string | null }));

vi.mock('../../../src/services/transport', async (importOriginal) => {
  const { transport } = await import('./settingsDaemon');
  return { ...(await importOriginal<Record<string, unknown>>()), getTransport: () => transport };
});
vi.mock('../../../src/services/transport/api/settings', async () => {
  const { transport } = await import('./settingsDaemon');
  return { getSettings: transport.getSettings, updateSettings: transport.updateSettings, installProfileTemplates: vi.fn() };
});
vi.mock('../../../src/services/transport/api/proxy', () => ({ getLoopGuardTrips: vi.fn(async () => []) }));
vi.mock('../../../src/services/platform', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isDesktop: () => desktop.is,
  pickFolder: vi.fn(async () => desktop.picked),
}));
vi.mock('../../../src/services/modelRegistry', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useModelLibrary: () => ({
    models: [
      { id: 3, name: 'three', quantization: 'Q4_K_M' },
      { id: 4, name: 'four' },
      { id: 5, name: 'flux', imageFamily: 'flux1' },
      { id: 6, name: 'sdxl', imageFamily: 'sdxl' },
    ],
    loading: false,
    error: null,
  }),
}));
vi.mock('../../../src/places/library/LibraryPlace', () => ({ default: () => <main aria-label="Library" /> }));

import { daemon, resetDaemon, transport, writes } from './settingsDaemon';
import { renderApp } from './harness';
import type { UpdateSettingsRequest } from '../../../src/types';
import { resolvedConfig } from '../fixtures/inference';

type User = ReturnType<typeof userEvent.setup>;
type Change = (user: User) => Promise<void>;

const field = (id: string) => {
  const element = document.querySelector<HTMLInputElement>(`#${id}`);
  if (!element) throw new Error(`no #${id} on the page`);
  return element;
};
const retype = (id: string, text: string): Change => async (user) => {
  await user.clear(field(id));
  if (text) await user.type(field(id), text);
  await user.tab();
};
const tick = (id: string): Change => async (user) => user.click(field(id));
const choose = (id: string, value: string): Change => async (user) => user.selectOptions(field(id), value);
const disclosed = (name: string, change: Change): Change => async (user) => {
  await user.click(screen.getByRole('button', { name }));
  await change(user);
};

/** Every stored field holds a value, so a field sent that was not changed fails the table. */
const STORED = {
  proxyPort: 8081,
  llamaBasePort: 9100,
  maxDownloadQueueSize: 5,
  defaultContextSize: 8192,
  shareLan: true,
  bindHost: '0.0.0.0',
  proxyHost: null,
  proxyAllowedHosts: ['desk.local'],
  proxyCacheOnDisk: null,
  proxyCacheDiskGb: null,
  proxyAutostart: true,
  startAtLogin: true,
  trustClientSampling: true,
  loopGuardMode: 'note' as const,
  titleGenerationPrompt: 'a prompt the user wrote',
  maxToolIterations: 12,
  agenticSampling: false,
  toolCallRepair: false,
  maxStagnationSteps: 7,
  defaultModelId: 3,
  defaultImageModelId: 5,
  mcpDrawing: true,
  defaultDownloadPath: '/downloads',
  inferenceDefaults: { temperature: 0.4, topK: 40 } as never,
};

beforeEach(() => {
  resetDaemon(STORED);
  desktop.is = false;
});

const changes: [string, string, Change, UpdateSettingsRequest][] = [
  ['general', 'start at login', tick('start-at-login-input'), { startAtLogin: false }],
  ['general', 'endpoint at launch', tick('proxy-autostart-input'), { proxyAutostart: false }],
  ['endpoint', 'port', retype('proxy-port-input', '8082'), { proxyPort: 8082 }],
  ['endpoint', 'port emptied', retype('proxy-port-input', ''), { proxyPort: null }],
  ['endpoint', 'LAN sharing', tick('share-lan-input'), { shareLan: false }],
  ['endpoint', 'bind address', retype('bind-host-input', '127.0.0.1'), { bindHost: '127.0.0.1' }],
  ['endpoint', 'endpoint address', retype('proxy-host-input', ' 0.0.0.0 '), { proxyHost: '0.0.0.0' }],
  ['endpoint', 'allowed hosts', retype('proxy-allowed-hosts-input', 'gglib.lan, ,192.168.1.20'), { proxyAllowedHosts: ['gglib.lan', '192.168.1.20'] }],
  ['endpoint', 'allowed hosts emptied', retype('proxy-allowed-hosts-input', ''), { proxyAllowedHosts: null }],
  ['endpoint', 'prompt cache on disk', tick('proxy-cache-on-disk-input'), { proxyCacheOnDisk: true }],
  ['endpoint', 'disk budget', retype('proxy-cache-disk-gb-input', '4'), { proxyCacheDiskGb: 4 }],
  ['endpoint', 'model servers start at', retype('server-port-input', '9200'), { llamaBasePort: 9200 }],
  ['endpoint', 'repair', disclosed('What gglib does to requests', tick('tool-call-repair-input')), { toolCallRepair: true }],
  ['endpoint', 'loop guard off', disclosed('What gglib does to requests', tick('loop-guard-input')), { loopGuardMode: 'off' }],
  ['endpoint', 'agent sampling cap', disclosed('What gglib does to requests', tick('agentic-sampling-input')), { agenticSampling: true }],
  ['endpoint', 'client sampling', disclosed('What gglib does to requests', tick('trust-client-sampling-input')), { trustClientSampling: false }],
  ['models', 'staging folder', retype('download-path-input', '/elsewhere'), { defaultDownloadPath: '/elsewhere' }],
  ['models', 'staging folder emptied', retype('download-path-input', ''), { defaultDownloadPath: null }],
  ['models', 'queue size', retype('max-queue-size-input', '6'), { maxDownloadQueueSize: 6 }],
  ['models', 'context', retype('context-size-input', '4096'), { defaultContextSize: 4096 }],
  ['models', 'model for gglib q', choose('default-model-select', '4'), { defaultModelId: 4 }],
  ['models', 'no model for gglib q', choose('default-model-select', ''), { defaultModelId: null }],
  ['models', 'image model', choose('default-image-model-select', '6'), { defaultImageModelId: 6 }],
  ['models', 'no image model', choose('default-image-model-select', ''), { defaultImageModelId: null }],
  ['tools', 'MCP drawing', tick('mcp-drawing-input'), { mcpDrawing: false }],
  ['tools', 'tool rounds', disclosed('Agent limits', retype('max-tool-iterations-input', '20')), { maxToolIterations: 20 }],
  ['tools', 'stuck steps', disclosed('Agent limits', retype('max-stagnation-steps-input', '9')), { maxStagnationSteps: 9 }],
  ['tools', 'title prompt', retype('title-prompt-input', 'another prompt'), { titleGenerationPrompt: 'another prompt' }],
  ['tools', 'title prompt Reset', async (user) => user.click(screen.getByRole('button', { name: 'Reset' })), { titleGenerationPrompt: null }],
];

describe('each field writes its own name, alone', () => {
  it.each(changes)('%s › %s', async (section, _name, change, sent) => {
    renderApp(`/settings/${section}`);
    await screen.findByRole('heading', { level: 2 });
    const user = userEvent.setup();
    await change(user);
    await waitFor(() => expect(writes()).toEqual([sent]));
  });
});

describe('the number fields', () => {
  it('put the default in the box and under it, announced with the field, within the range the backend takes', async () => {
    resetDaemon();
    renderApp('/settings/endpoint');
    const port = await screen.findByLabelText('Port');
    expect(port).toHaveAttribute('placeholder', '8080');
    expect(port).toHaveAttribute('min', '1024');
    expect(port).toHaveAttribute('max', '65535');
    const hint = document.getElementById(port.getAttribute('aria-describedby') ?? '');
    expect(hint).toHaveTextContent('Default: 8080');
  });

  it('say what an empty box does where there is no one default', async () => {
    resetDaemon();
    renderApp('/settings/models');
    const context = await screen.findByLabelText('Context for models without their own');
    expect(context).toHaveAttribute('placeholder', 'auto');
    const hint = document.getElementById(context.getAttribute('aria-describedby') ?? '');
    expect(hint).toHaveTextContent(/the server sizes each launch/);
    expect(hint).not.toHaveTextContent('Default:');
  });
});

describe('the models folder', () => {
  it('Reset writes the default folder alone, and no setting', async () => {
    daemon.folder = { ...daemon.folder, path: '/custom/models', source: 'explicit' };
    renderApp('/settings/models');
    const user = userEvent.setup();
    expect(await screen.findByLabelText('Models folder')).toHaveValue('/custom/models');
    await user.click(screen.getByRole('button', { name: 'Reset' }));
    await waitFor(() => expect(transport.setModelsDirectory).toHaveBeenCalledWith('/models'));
    expect(transport.setModelsDirectory).toHaveBeenCalledTimes(1);
    expect(writes()).toEqual([]);
    expect(await screen.findByText('exists · writable')).toBeInTheDocument();
    await user.click(await screen.findByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(transport.setModelsDirectory).toHaveBeenLastCalledWith('/custom/models'));
  });

  /** The models folder's field: its box, its hint row's actions, its lines. */
  const folder = async () => within((await screen.findByLabelText('Models folder')).closest('div.flex-col') as HTMLElement);

  it('is typed on the web and picked with Change… in the desktop app', async () => {
    const web = renderApp('/settings/models');
    expect((await folder()).queryByRole('button', { name: 'Change…' })).not.toBeInTheDocument();
    web.unmount();
    desktop.is = true;
    desktop.picked = '/Volumes/big/models';
    renderApp('/settings/models');
    await userEvent.setup().click((await folder()).getByRole('button', { name: 'Change…' }));
    await waitFor(() => expect(transport.setModelsDirectory).toHaveBeenCalledWith('/Volumes/big/models'));
  });

  it('says a folder that is not there yet, or cannot be written', async () => {
    daemon.folder = { ...daemon.folder, exists: false, writable: false };
    renderApp('/settings/models');
    expect(await screen.findByText('will be created · not writable')).toBeInTheDocument();
  });
});

describe('Stop runaway loops', () => {
  it('ticking it from off writes note', async () => {
    resetDaemon({ loopGuardMode: 'off' });
    renderApp('/settings/endpoint');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'What gglib does to requests' }));
    expect(field('loop-guard-input')).not.toBeChecked();
    await user.click(field('loop-guard-input'));
    await waitFor(() => expect(writes()).toEqual([{ loopGuardMode: 'note' }]));
  });

  it("shows the terminal's Refuse as on, says so, and Undo after unticking puts Refuse back", async () => {
    resetDaemon({ loopGuardMode: 'refuse' });
    renderApp('/settings/endpoint');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'What gglib does to requests' }));
    expect(field('loop-guard-input')).toBeChecked();
    const said = screen.getByText(/^Refusing, as/);
    expect(said).toHaveTextContent('Refusing, as gglib config settings set --loop-guard-mode refuse set it: a looping request is answered with a 400.');
    await user.click(field('loop-guard-input'));
    await user.click(await screen.findByRole('button', { name: 'Undo' }));
    await waitFor(() => expect(writes()).toEqual([{ loopGuardMode: 'off' }, { loopGuardMode: 'refuse' }]));
  });
});

describe('the model choices', () => {
  it('list every model for gglib q, and only the models that draw for drawing', async () => {
    renderApp('/settings/models');
    const q = within(await screen.findByLabelText('Model for gglib q'));
    expect(q.getAllByRole('option').map((o) => o.textContent)).toEqual(['None', 'three (Q4_K_M)', 'four', 'flux', 'sdxl']);
    const drawing = within(screen.getByLabelText('Image model for drawing'));
    expect(drawing.getAllByRole('option').map((o) => o.textContent)).toEqual(['None', 'flux', 'sdxl']);
    expect(screen.getByLabelText('Image model for drawing')).toHaveValue('5');
  });

  it('keep a stored image model that is no image model here as its own choice', async () => {
    resetDaemon({ defaultImageModelId: 4 });
    renderApp('/settings/models');
    const drawing = await screen.findByLabelText('Image model for drawing');
    expect(drawing).toHaveValue('4');
    expect(within(drawing).getByRole('option', { name: 'Model 4 (not an image model in this library)' })).toBeInTheDocument();
    expect(writes()).toEqual([]);
  });
});

describe('Sampling defaults', () => {
  it('says what is set on one line, and Change… saves the defaults alone', async () => {
    renderApp('/settings/models');
    const user = userEvent.setup();
    const sampling = within(await screen.findByRole('region', { name: 'Sampling defaults' }));
    expect(sampling.getByText(/temperature=0\.4/)).toBeInTheDocument();
    await user.click(sampling.getByRole('button', { name: 'Change…' }));
    await user.clear(field('inference-param-topK'));
    await user.type(field('inference-param-topK'), '50');
    await user.click(sampling.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(writes()).toEqual([{ inferenceDefaults: { temperature: 0.4, topK: 50 } }]));
    expect(sampling.getByText(/top-k=50/)).toBeInTheDocument();
    expect(sampling.queryByRole('button', { name: 'Save' })).not.toBeInTheDocument();
  });

  it('sends nothing for a Cancel, nor for a Save of what is stored', async () => {
    // As the daemon answers: every key named, the unset ones `null`.
    resetDaemon({ inferenceDefaults: resolvedConfig({ temperature: 0.4, topK: 40 }) });
    renderApp('/settings/models');
    const user = userEvent.setup();
    const sampling = within(await screen.findByRole('region', { name: 'Sampling defaults' }));
    await user.click(sampling.getByRole('button', { name: 'Change…' }));
    await user.click(sampling.getByRole('button', { name: 'Cancel' }));
    await user.click(sampling.getByRole('button', { name: 'Change…' }));
    await user.click(sampling.getByRole('button', { name: 'Save' }));
    expect(writes()).toEqual([]);
  });

  it('says None set when no default is stored', async () => {
    resetDaemon();
    renderApp('/settings/models');
    expect(within(await screen.findByRole('region', { name: 'Sampling defaults' })).getByText('None set')).toBeInTheDocument();
  });
});

describe('Profiles in the place', () => {
  it('adds one through its sheet, writing the profile list', async () => {
    renderApp('/settings/models');
    const user = userEvent.setup();
    const profiles = within(await screen.findByRole('region', { name: 'Profiles' }));
    await user.click(await profiles.findByRole('button', { name: 'Add profile' }));
    const sheet = within(await screen.findByRole('dialog', { name: 'Add profile' }));
    await user.type(sheet.getByLabelText(/^name$/i), 'chat');
    await user.click(sheet.getByRole('button', { name: /create profile/i }));
    await waitFor(() => expect(writes()).toEqual([{ inferenceProfiles: [expect.objectContaining({ name: 'chat', config: {} })] }]));
    expect(await profiles.findByText('chat')).toBeInTheDocument();
  });
});

describe('the menu bar toggle', () => {
  it('is drawn in the desktop app only', async () => {
    const web = renderApp('/settings/general');
    await screen.findByRole('checkbox', { name: /^Start gglib at login/ });
    expect(document.querySelector('#close-to-tray-input')).toBeNull();
    web.unmount();
    desktop.is = true;
    renderApp('/settings/general');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('checkbox', { name: /^Keep running in the menu bar when the window closes/ }));
    await waitFor(() => expect(writes()).toEqual([{ closeToTray: true }]));
  });
});

describe('sameValue', () => {
  it('reads a key set to null and a key left out as the same unset value, and compares the rest by content', async () => {
    const { sameValue } = await import('../../../src/places/settings/useFieldSave');
    // The daemon names every sampling key, `null` where unset; the form drops a key it clears.
    expect(sameValue({ temperature: 0.4, minP: null }, { temperature: 0.4 })).toBe(true);
    expect(sameValue(['a', 'b'], ['a', 'b'])).toBe(true);
    expect(sameValue({ temperature: 0.4 }, { temperature: 0.5 })).toBe(false);
    expect(sameValue(['a'], ['a', 'b'])).toBe(false);
    expect(sameValue(null, false)).toBe(false);
  });
});
