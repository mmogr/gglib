/**
 * The settings place's fields, each through the real routes: every field
 * the place rebuilt writes its binding's own name and no other field (ST-1,
 * ST-7), with the value it means; a number's default in the box and under
 * it, announced with the field (ST-2), and the range the backend accepts
 * (ST-3); the models folder's Reset writing the folder alone (ST-4); the
 * loop guard writing true and false; the image model
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
  proxyHost: null,
  proxyAllowedHosts: ['desk.local'],
  proxyCacheOnDisk: null,
  proxyCacheDiskGb: null,
  proxyAutostart: true,
  startAtLogin: true,
  trustClientSampling: true,
  loopGuard: null,
  titleGenerationPrompt: 'a prompt the user wrote',
  maxToolIterations: 12,
  toolCallRepair: false,
  maxStagnationSteps: 7,
  defaultModelId: 3,
  defaultImageModelId: 5,
  mcpDrawing: true,
  inferenceDefaults: { temperature: 0.4, topK: 40 } as never,
};

beforeEach(() => {
  resetDaemon(STORED);
  desktop.is = false;
});

const changes: [string, string, Change, UpdateSettingsRequest][] = [
  ['general', 'start at login', tick('start-at-login-input'), { startAtLogin: false }],
  ['general', 'endpoint at launch', tick('proxy-autostart-input'), { proxyAutostart: false }],
  ['general', 'menu bar', tick('close-to-tray-input'), { closeToTray: true }],
  ['endpoint', 'port', retype('proxy-port-input', '8082'), { proxyPort: 8082 }],
  ['endpoint', 'port emptied', retype('proxy-port-input', ''), { proxyPort: null }],
  ['endpoint', 'bind address', retype('proxy-host-input', ' 0.0.0.0 '), { proxyHost: '0.0.0.0' }],
  ['endpoint', 'allowed hosts', retype('proxy-allowed-hosts-input', 'gglib.lan, ,192.168.1.20'), { proxyAllowedHosts: ['gglib.lan', '192.168.1.20'] }],
  ['endpoint', 'allowed hosts emptied', retype('proxy-allowed-hosts-input', ''), { proxyAllowedHosts: null }],
  ['endpoint', 'prompt cache on disk', tick('proxy-cache-on-disk-input'), { proxyCacheOnDisk: true }],
  ['endpoint', 'disk budget', retype('proxy-cache-disk-gb-input', '4'), { proxyCacheDiskGb: 4 }],
  ['endpoint', 'model servers start at', retype('server-port-input', '9200'), { llamaBasePort: 9200 }],
  ['endpoint', 'repair', disclosed('What gglib does to requests', tick('tool-call-repair-input')), { toolCallRepair: true }],
  ['endpoint', 'loop guard off', disclosed('What gglib does to requests', tick('loop-guard-input')), { loopGuard: false }],
  ['endpoint', 'client sampling', disclosed('What gglib does to requests', tick('trust-client-sampling-input')), { trustClientSampling: false }],
  ['models', 'queue size', retype('max-queue-size-input', '6'), { maxDownloadQueueSize: 6 }],
  ['models', 'context', retype('context-size-input', '4096'), { defaultContextSize: 4096 }],
  ['models', 'model for gglib q', choose('default-model-select', '4'), { defaultModelId: 4 }],
  ['models', 'no model for gglib q', choose('default-model-select', ''), { defaultModelId: null }],
  ['models', 'image model', choose('default-image-model-select', '6'), { defaultImageModelId: 6 }],
  ['models', 'no image model', choose('default-image-model-select', ''), { defaultImageModelId: null }],
  ['tools', 'MCP drawing', tick('mcp-drawing-input'), { mcpDrawing: false }],
  ['tools', 'tool rounds', disclosed('Agent limits', retype('max-tool-iterations-input', '20')), { maxToolIterations: 20 }],
  ['tools', 'stuck steps', disclosed('Agent limits', retype('max-stagnation-steps-input', '9')), { maxStagnationSteps: 9 }],
  ['tools', 'parallel tool calls', disclosed('Agent limits', retype('max-parallel-tools-input', '4')), { maxParallelTools: 4 }],
  ['tools', 'tool timeout', disclosed('Agent limits', retype('tool-timeout-ms-input', '5000')), { toolTimeoutMs: 5000 }],
  ['tools', 'title prompt', retype('title-prompt-input', 'another prompt'), { titleGenerationPrompt: 'another prompt' }],
  ['tools', 'title prompt Reset', async (user) => user.click(screen.getByRole('button', { name: 'Reset' })), { titleGenerationPrompt: null }],
];

describe('each field writes its own name, alone', () => {
  // The desktop app, which draws every field.
  beforeEach(() => {
    desktop.is = true;
  });

  it.each(changes)('%s › %s', async (section, _name, change, sent) => {
    renderApp(`/settings/${section}`);
    await screen.findByRole('heading', { level: 2 });
    const user = userEvent.setup();
    await change(user);
    await waitFor(() => expect(writes()).toEqual([sent]));
  });
});

/** A field's whole block: its label, box, hint row, note and save line. */
const block = (id: string) => field(id).closest('div.flex-col') as HTMLElement;

describe('the number fields', () => {
  // [section, field, its disclosure, placeholder, what the hint says, min, max]
  const NUMBERS: [string, string, string | null, string, string | RegExp, string, string | null][] = [
    ['endpoint', 'proxy-port-input', null, '8080', 'Default: 8080', '1024', '65535'],
    ['endpoint', 'server-port-input', null, '9000', 'Default: 9000', '1024', '65535'],
    ['endpoint', 'proxy-cache-disk-gb-input', null, 'auto', 'Left empty, it is sized from free disk.', '1', null],
    ['models', 'max-queue-size-input', null, '10', 'Default: 10', '1', '50'],
    ['models', 'context-size-input', null, 'auto', /the server sizes each launch/, '512', '1000000'],
    ['tools', 'max-tool-iterations-input', 'Agent limits', '25', 'Default: 25', '1', '50'],
    ['tools', 'max-stagnation-steps-input', 'Agent limits', '5', 'Default: 5', '1', '100'],
    ['tools', 'max-parallel-tools-input', 'Agent limits', '25', 'Default: 25', '1', '50'],
    ['tools', 'tool-timeout-ms-input', 'Agent limits', '30000', 'Default: 30000', '100', '60000'],
  ];

  it.each(NUMBERS)(
    '%s › %s: the default in the box and under it, announced with it, within the range the backend takes',
    async (section, id, disclosure, placeholder, hint, min, max) => {
      resetDaemon();
      renderApp(`/settings/${section}`);
      await screen.findByRole('heading', { level: 2 });
      if (disclosure) await userEvent.setup().click(await screen.findByRole('button', { name: disclosure }));
      await waitFor(() => field(id));
      const input = field(id);
      expect(input).toHaveAttribute('placeholder', placeholder);
      expect(input).toHaveAttribute('min', min);
      if (max === null) expect(input).not.toHaveAttribute('max');
      else expect(input).toHaveAttribute('max', max);
      const described = document.getElementById(input.getAttribute('aria-describedby') ?? '');
      expect(described).toHaveTextContent(hint);
      if (typeof hint !== 'string' || !hint.startsWith('Default:')) expect(described).not.toHaveTextContent('Default:');
    },
  );
});

describe('when a change takes effect', () => {
  const ENDPOINT = 'Takes effect when the endpoint next turns on.';
  const GGLIB = 'Takes effect when gglib next starts.';
  const NEXT_LOAD =
    'Takes effect for the next model loaded. A model the endpoint loads for a client keeps the value the endpoint turned on with.';
  const DESKTOP = 'Takes effect when the desktop app next starts.';
  const KEY = 'A new key takes effect within a few seconds. A cleared key stops being required when the endpoint next turns on.';
  // [section, field, its disclosure, its note or null for one read where it is used]
  const NOTES: [string, string, string | null, string | null][] = [
    ['general', 'start-at-login-input', null, DESKTOP],
    ['general', 'proxy-autostart-input', null, GGLIB],
    ['general', 'close-to-tray-input', null, null],
    ['endpoint', 'proxy-port-input', null, ENDPOINT],
    ['endpoint', 'proxy-api-key-input', null, KEY],
    ['endpoint', 'proxy-host-input', null, ENDPOINT],
    ['endpoint', 'proxy-allowed-hosts-input', null, ENDPOINT],
    ['endpoint', 'proxy-cache-on-disk-input', null, ENDPOINT],
    ['endpoint', 'proxy-cache-disk-gb-input', null, ENDPOINT],
    ['endpoint', 'server-port-input', null, GGLIB],
    ['endpoint', 'tool-call-repair-input', 'What gglib does to requests', null],
    ['endpoint', 'loop-guard-input', 'What gglib does to requests', null],
    ['endpoint', 'trust-client-sampling-input', 'What gglib does to requests', null],
    ['models', 'models-dir-input', null, null],
    ['models', 'max-queue-size-input', null, null],
    ['models', 'context-size-input', null, NEXT_LOAD],
    ['models', 'default-model-select', null, null],
    ['models', 'default-image-model-select', null, null],
    ['tools', 'mcp-drawing-input', null, null],
    ['tools', 'max-tool-iterations-input', 'Agent limits', null],
    ['tools', 'max-stagnation-steps-input', 'Agent limits', null],
    ['tools', 'max-parallel-tools-input', 'Agent limits', null],
    ['tools', 'tool-timeout-ms-input', 'Agent limits', null],
    ['tools', 'title-prompt-input', null, null],
  ];

  beforeEach(() => {
    desktop.is = true;
  });

  it.each(NOTES)('%s › %s', async (section, id, disclosure, note) => {
    renderApp(`/settings/${section}`);
    await screen.findByRole('heading', { level: 2 });
    if (disclosure) await userEvent.setup().click(await screen.findByRole('button', { name: disclosure }));
    await waitFor(() => field(id));
    const said = within(block(id)).queryAllByText(/takes effect|stops being required/i).map((p) => p.textContent);
    expect(said).toEqual(note === null ? [] : [note]);
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
    expect(await screen.findByText('Writable')).toBeInTheDocument();
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

  it('says a folder that is not there yet, or cannot be written, in the served word with the attention mark', async () => {
    daemon.folder = { ...daemon.folder, exists: false, writable: true, state: 'absent' };
    const first = renderApp('/settings/models');
    const absent = await screen.findByText('Will be created');
    expect(absent.closest('[data-kind]')).toHaveAttribute('data-kind', 'attention');
    expect(screen.queryByText(/not writable/i)).not.toBeInTheDocument();
    first.unmount();

    daemon.folder = { ...daemon.folder, exists: true, writable: false, state: 'not_writable' };
    renderApp('/settings/models');
    const locked = await screen.findByText('Not writable');
    expect(locked.closest('[data-kind]')).toHaveAttribute('data-kind', 'attention');
  });

  it('says a folder that is there and writable plainly, with no mark', async () => {
    renderApp('/settings/models');
    const said = await screen.findByText('Writable');
    expect(said.closest('[data-kind]')).toBeNull();
  });
});

describe('Stop repeated calls', () => {
  it('ticking it from off writes true', async () => {
    resetDaemon({ loopGuard: false });
    renderApp('/settings/endpoint');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'What gglib does to requests' }));
    expect(field('loop-guard-input')).not.toBeChecked();
    await user.click(field('loop-guard-input'));
    await waitFor(() => expect(writes()).toEqual([{ loopGuard: true }]));
  });
});

describe('the title prompt', () => {
  it('puts the stored prompt back on Escape, and then writes nothing when left', async () => {
    renderApp('/settings/tools');
    const user = userEvent.setup();
    const prompt = await screen.findByLabelText('Title prompt');
    await user.type(prompt, ' and more');
    expect(prompt).toHaveValue('a prompt the user wrote and more');
    await user.keyboard('{Escape}');
    expect(prompt).toHaveValue('a prompt the user wrote');
    await user.tab();
    expect(writes()).toEqual([]);
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

  it('writes nothing for a Save with nothing stored and nothing set', async () => {
    resetDaemon();
    renderApp('/settings/models');
    const user = userEvent.setup();
    const sampling = within(await screen.findByRole('region', { name: 'Sampling defaults' }));
    await user.click(sampling.getByRole('button', { name: 'Change…' }));
    await user.click(sampling.getByRole('button', { name: 'Save' }));
    expect(sampling.queryByRole('button', { name: 'Save' })).not.toBeInTheDocument();
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
  it('and the login item are drawn in the desktop app only', async () => {
    const web = renderApp('/settings/general');
    await screen.findByRole('checkbox', { name: /^Start the endpoint when gglib launches/ });
    expect(document.querySelector('#close-to-tray-input')).toBeNull();
    expect(document.querySelector('#start-at-login-input')).toBeNull();
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
