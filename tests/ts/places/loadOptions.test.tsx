/**
 * The Load with options sheet, driven through the real routes: each opener
 * opens it (⋯ Load with options…, ⋯ Load and pin, `?serve=1`, a launch-line
 * token's Change for this model…); each field's placeholder and caption is
 * the explanation's; Load sends only what was typed, the pin first when
 * asked, and opens no chat; with This load only unchecked the typed fields
 * are saved first, each over the stored rest, and checked nothing is saved;
 * Speculative only for a model with MTP heads; Port collapsed; a failed load
 * stays in the sheet; an image model's Parts line and its runtime's Install.
 *
 * The transport is a fake; a serve and a pin go through the real API calls
 * and mapper to a fake `post`, so each body is the JSON a load sends.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, cleanup, fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

const library = vi.hoisted(() => ({ models: [] as unknown[] }));
const explain = vi.hoisted(() => ({ answer: null as unknown }));
const settings = vi.hoisted(() => ({ value: {} as Record<string, unknown> }));
const sd = vi.hoisted(() => ({ installed: true }));
const post = vi.hoisted(() => vi.fn(async (_path: string, _body?: unknown): Promise<unknown> => ({ port: 9456 })));
const install = vi.hoisted(() => ({ kinds: [] as string[], onEvent: {} as Record<string, (e: unknown) => void> }));
const setup = vi.hoisted(() => ({ llamaInstalled: true }));
const transport = vi.hoisted(() => ({
  listModels: vi.fn(async () => library.models),
  listServers: vi.fn(async () => []),
  stopServer: vi.fn(async (_id: number) => {}),
  removeModel: vi.fn(async (_id: number) => {}),
  updateModel: vi.fn(async (_p: Record<string, unknown>) => ({})),
  getDownloadQueue: vi.fn(async () => null),
  listPairedModels: vi.fn(async () => null),
  getModelDetail: vi.fn(async (): Promise<unknown> => null),
  addModelTag: vi.fn(async (_id: number, _tag: string) => {}),
  removeModelTag: vi.fn(async (_id: number, _tag: string) => {}),
}));

vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  const { serveModel } = await import('../../../src/services/transport/api/servers');
  const { startPinnedProxy } = await import('../../../src/services/transport/api/proxy');
  return {
    ...actual,
    getTransport: () => ({
      ...transport,
      serveModel,
      startPinnedProxy,
      getSettings: vi.fn(async () => settings.value),
      subscribe: vi.fn(() => () => {}),
      onEventStreamOpen: vi.fn(() => () => {}),
      revealSecret: vi.fn(async () => null),
    }),
  };
});
vi.mock('../../../src/services/transport/api/client', () => ({
  get: vi.fn(async (path: string) => (path.includes('/explain') ? explain.answer : null)),
  post: (path: string, body?: unknown) => post(path, body),
}));
vi.mock('../../../src/services/transport/api/setup', () => ({
  getSetupStatus: vi.fn(async () => ({ llamaInstalled: setup.llamaInstalled, llamaCanDownload: true })),
  getEngineStatus: vi.fn(async () => ({ installed: sd.installed, installCommand: 'gglib config sd install', install: null })),
  streamEngineInstall: vi.fn((kind: string, onEvent: (e: unknown) => void) => {
    install.kinds.push(kind);
    install.onEvent[kind] = onEvent;
    return () => {};
  }),
  getRecommendedModel: vi.fn(async () => null),
}));
vi.mock('../../../src/services/remoteEvents', () => ({ refreshRemoteStatus: vi.fn() }));
vi.mock('../../../src/pages/ChatPage', () => ({
  default: () => <div data-testid="chat-page" />,
}));

import { cleanupModelEvents, initModelEvents } from '../../../src/services/modelEvents';
import { resetEngineInstall } from '../../../src/places/library/engineInstall';
import { INFERENCE_CONFIG_KEYS, INFERENCE_PARAMS } from '../../../src/constants/inferenceDefaults';
import type { GgufModel } from '../../../src/types';
import { renderApp } from './harness';
import { guiModel } from '../fixtures/model';
import { chatPlan, launchExplanation, samplingExplanation } from '../fixtures/explain';
import { resolvedConfig } from '../fixtures/inference';

const GEMMA = guiModel({ id: 9, name: 'Gemma-3-4B-It', openaiId: 'Gemma-3-4B-It', contextLength: 262144 });

/** The plan's drafting arguments, as an `mtp`-tagged model's launch has them. */
const MTP_ARGS = [
  { flag: '--spec-type', value: 'draft-mtp', decision: 'mtp', source: 'gglib' as const, reason: 'tagged mtp: the model drafts its own tokens ahead' },
  { flag: '--spec-draft-n-max', value: '2', decision: 'mtp', source: 'gglib' as const, reason: 'tagged mtp: the model drafts its own tokens ahead' },
  { flag: '--spec-draft-p-min', value: '0.75', decision: 'mtp', source: 'gglib' as const, reason: 'tagged mtp: the model drafts its own tokens ahead' },
];

function renderInspector(model: GgufModel = GEMMA, path = `/models/${model.id}`) {
  library.models = [model];
  initModelEvents();
  return renderApp(path);
}

const user = () => userEvent.setup();
const sheet = () => screen.getByRole('dialog', { name: 'Load with options' });

async function openSheet(item: 'Load with options…' | 'Load and pin' = 'Load with options…') {
  const u = user();
  await u.click(await screen.findByRole('button', { name: 'More' }));
  await u.click(screen.getByRole('menuitem', { name: item }));
  return { u, dialog: await screen.findByRole('dialog', { name: 'Load with options' }) };
}

/** Each body `post` was handed, parsed, by path, in order. */
const bodies = () => post.mock.calls.map(([path, body]) => [path, JSON.parse(JSON.stringify(body))] as const);

describe('the Load with options sheet', () => {
  beforeEach(() => {
    explain.answer = launchExplanation(samplingExplanation(), chatPlan());
    settings.value = { llamaBasePort: 9100 };
    sd.installed = true;
    setup.llamaInstalled = true;
    install.kinds = [];
    install.onEvent = {};
    post.mockClear();
    post.mockImplementation(async () => ({ port: 9456 }));
    for (const mock of Object.values(transport)) mock.mockClear();
    transport.getModelDetail.mockResolvedValue(null);
    resetEngineInstall();
  });
  afterEach(() => {
    cleanup();
    cleanupModelEvents();
  });

  it('opens from ⋯ Load with options…, Pin unchecked, and Cancel puts the inspector back', async () => {
    renderInspector();
    const { u, dialog } = await openSheet();

    expect(within(dialog).getByRole('checkbox', { name: /Pin the endpoint to this model/ })).not.toBeChecked();
    await u.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByRole('dialog', { name: 'Load with options' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Load' })).toBeInTheDocument();
  });

  it('opens from ⋯ Load and pin with Pin checked, and pins before it loads', async () => {
    renderInspector();
    const { u, dialog } = await openSheet('Load and pin');

    expect(within(dialog).getByRole('checkbox', { name: /Pin the endpoint to this model/ })).toBeChecked();
    await u.click(within(dialog).getByRole('button', { name: 'Load' }));

    await waitFor(() => expect(post).toHaveBeenCalledTimes(2));
    expect(bodies()).toEqual([
      ['/api/proxy/start-pinned', { model_id: 9, options: { mlock: false } }],
      ['/api/servers/start', { id: 9, mlock: false }],
    ]);
  });

  it('opens from a cold ?serve=1 (the native menu’s Serve), then strips it', async () => {
    const { at } = renderInspector(GEMMA, '/models/9?serve=1');

    expect(await screen.findByRole('dialog', { name: 'Load with options' })).toBeInTheDocument();
    await waitFor(() => expect(at()).toBe('/models/9'));
  });

  it('shows what each field would otherwise use, from the explanation', async () => {
    const fit = chatPlan().fit!;
    explain.answer = launchExplanation(
      samplingExplanation({ resolved: resolvedConfig({ topK: 64 }), sources: [{ param: 'topK', kind: 'layer', layer: 'modelUserSet' }] }),
      chatPlan({ fit: { ...fit, servesAt: 8192 } }),
    );
    renderInspector();
    const { dialog } = await openSheet();

    const context = within(dialog).getByRole('spinbutton', { name: 'Context' });
    expect(context).toHaveAttribute('placeholder', '8,192');
    // Never the trained window the model records.
    expect(context.getAttribute('placeholder')).not.toContain('262,144');
    expect(within(dialog).getByText('gglib: the largest rung that fits beside the weights on this machine')).toBeInTheDocument();
    expect(within(dialog).getByText("Auto: --jinja · gglib: tagged agent: tool calls need the model's own template")).toBeInTheDocument();
    expect(within(dialog).getByLabelText('Top K')).toHaveAttribute('placeholder', '64');
    expect(within(dialog).getByText('Resolves to 64 — per-model defaults (user-set)')).toBeInTheDocument();
  });

  it('keeps Port collapsed, reading Auto from the base port', async () => {
    renderInspector();
    const { u, dialog } = await openSheet();

    const port = within(dialog).getByText('Port').closest('details')!;
    expect(port).not.toHaveAttribute('open');
    expect(within(port).getByText('Auto (from 9100)')).toBeInTheDocument();
    await u.click(within(port).getByText('Port'));
    expect(port).toHaveAttribute('open');
  });

  it('loads with only what was typed, saving nothing for this load only, and opens no chat', async () => {
    renderInspector();
    const { u, dialog } = await openSheet();

    await u.type(within(dialog).getByRole('spinbutton', { name: 'Context' }), '16384');
    await u.selectOptions(within(dialog).getByRole('combobox', { name: 'Jinja' }), 'Off');
    await u.type(within(dialog).getByLabelText('Top K'), '40');
    await u.click(within(dialog).getByText('Port'));
    await u.type(within(dialog).getByRole('spinbutton', { name: 'Port' }), '9300');
    await u.click(within(dialog).getByRole('checkbox', { name: /This load only/ }));
    await u.click(within(dialog).getByRole('button', { name: 'Load' }));

    await waitFor(() => expect(post).toHaveBeenCalledTimes(1));
    expect(bodies()).toEqual([
      ['/api/servers/start', { id: 9, mlock: false, contextLength: 16384, port: 9300, jinja: false, inferenceParams: { topK: 40 } }],
    ]);
    expect(transport.updateModel).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Load with options' })).not.toBeInTheDocument());
    expect(screen.queryByTestId('chat-page')).not.toBeInTheDocument();
  });

  it('sends no Jinja key on Auto, and nothing at all when nothing is typed', async () => {
    renderInspector();
    const { u, dialog } = await openSheet();
    const jinja = within(dialog).getByRole('combobox', { name: 'Jinja' });
    await u.selectOptions(jinja, 'On');
    await u.selectOptions(jinja, 'Auto');
    await u.click(within(dialog).getByRole('button', { name: 'Load' }));

    await waitFor(() => expect(post).toHaveBeenCalledTimes(1));
    expect(JSON.stringify(post.mock.calls[0][1])).toBe('{"id":9,"mlock":false}');
    expect(transport.updateModel).not.toHaveBeenCalled();
  });

  it("saves only the typed fields as the model's own, over the stored rest, then loads", async () => {
    const stored = resolvedConfig({ temperature: 0.7 });
    renderInspector(guiModel({ ...GEMMA, serverDefaults: { contextLength: null, jinja: true }, inferenceDefaults: stored }));
    const { u, dialog } = await openSheet();

    await u.type(within(dialog).getByRole('spinbutton', { name: 'Context' }), '16384');
    await u.type(within(dialog).getByLabelText('Top K'), '40');
    await u.click(within(dialog).getByText('Port'));
    await u.type(within(dialog).getByRole('spinbutton', { name: 'Port' }), '9300');
    await u.click(within(dialog).getByRole('button', { name: 'Load' }));

    await waitFor(() => expect(post).toHaveBeenCalledTimes(1));
    expect(transport.updateModel).toHaveBeenCalledTimes(1);
    const sent = Object.fromEntries(Object.entries(transport.updateModel.mock.calls[0][0]).filter(([, v]) => v !== undefined));
    expect(sent).toEqual({ id: 9, serverDefaults: { contextLength: 16384, jinja: true }, inferenceDefaults: { ...stored, topK: 40 } });
    expect(transport.updateModel.mock.invocationCallOrder[0]).toBeLessThan(post.mock.invocationCallOrder[0]);
    expect(bodies()[0][1]).toEqual({ id: 9, mlock: false, contextLength: 16384, port: 9300, inferenceParams: { topK: 40 } });
  });

  it('keeps a failed save in the sheet and loads nothing', async () => {
    transport.updateModel.mockRejectedValueOnce(new Error('the database is locked'));
    renderInspector();
    const { u, dialog } = await openSheet();
    await u.type(within(dialog).getByRole('spinbutton', { name: 'Context' }), '16384');
    await u.click(within(dialog).getByRole('button', { name: 'Load' }));

    expect(await within(dialog).findByRole('alert')).toHaveTextContent("Didn't save: the database is locked");
    expect(post).not.toHaveBeenCalled();
  });

  it('keeps a failed load in the sheet, in the daemon’s words', async () => {
    post.mockRejectedValueOnce(new Error('out of memory (needs 9.1 GB, 6.0 free)'));
    renderInspector();
    const { u, dialog } = await openSheet();
    await u.click(within(dialog).getByRole('button', { name: 'Load' }));

    expect(await within(dialog).findByRole('alert')).toHaveTextContent("Didn't load: out of memory (needs 9.1 GB, 6.0 free)");
    expect(sheet()).toBeInTheDocument();
  });

  it('offers Speculative only for a model with MTP heads, its fields reading the plan', async () => {
    renderInspector();
    let { dialog } = await openSheet();
    expect(within(dialog).queryByRole('group', { name: 'Speculative' })).not.toBeInTheDocument();
    cleanup();
    cleanupModelEvents();

    transport.getModelDetail.mockResolvedValue({ id: 9, tags: ['mtp'] });
    explain.answer = launchExplanation(samplingExplanation(), chatPlan({ args: [...chatPlan().args, ...MTP_ARGS] }));
    renderInspector();
    await waitFor(() => expect(transport.getModelDetail).toHaveBeenCalled());
    ({ dialog } = await openSheet());
    const spec = await within(dialog).findByRole('group', { name: 'Speculative' });
    expect(within(spec).getByRole('checkbox', { name: 'MTP' })).toBeChecked();
    expect(within(spec).getByRole('spinbutton', { name: 'Draft tokens' })).toHaveAttribute('placeholder', '2');
    expect(within(spec).getByRole('spinbutton', { name: 'Min probability' })).toHaveAttribute('placeholder', '0.75');
  });

  it.each([
    { name: 'turned off', act: 'off', body: { mtpDraftNMax: 0 } },
    { name: 'a count typed', act: 'count', body: { mtpDraftNMax: 4, mtpDraftPMin: 0.6 } },
  ])('sends the drafting $name, and saves MTP on or off as the model’s own', async ({ act, body }) => {
    transport.getModelDetail.mockResolvedValue({ id: 9, tags: ['mtp'] });
    explain.answer = launchExplanation(samplingExplanation(), chatPlan({ args: [...chatPlan().args, ...MTP_ARGS] }));
    renderInspector();
    await waitFor(() => expect(transport.getModelDetail).toHaveBeenCalled());
    const { u, dialog } = await openSheet();
    const spec = await within(dialog).findByRole('group', { name: 'Speculative' });
    if (act === 'off') {
      await u.click(within(spec).getByRole('checkbox', { name: 'MTP' }));
    } else {
      await u.type(within(spec).getByRole('spinbutton', { name: 'Draft tokens' }), '4');
      await u.type(within(spec).getByRole('spinbutton', { name: 'Min probability' }), '0.6');
    }
    await u.click(within(dialog).getByRole('button', { name: 'Load' }));

    await waitFor(() => expect(post).toHaveBeenCalledTimes(1));
    expect(bodies()[0][1]).toEqual({ id: 9, mlock: false, ...body });
    // Only on or off has a per-model home; the counts ride the launch alone.
    if (act === 'off') expect(transport.updateModel.mock.calls[0][0]).toMatchObject({ serverDefaults: { mtp: false } });
    else expect(transport.updateModel).not.toHaveBeenCalled();
  });

  it('opens on the field of a launch-line token’s Change for this model…, and underlines the tokens you chose', async () => {
    renderInspector();
    const u = user();
    const line = await screen.findByLabelText('Launch line');
    expect(within(line).getByRole('button', { name: '--mmproj /models/mmproj-F16.gguf' })).toHaveClass('underline');
    expect(within(line).getByRole('button', { name: '-c 32768' })).not.toHaveClass('underline');

    // The weights are no field of the sheet's: no Change.
    await u.click(within(line).getByRole('button', { name: '-m /models/gemma.gguf' }));
    expect(within(screen.getByRole('note', { name: '-m' })).queryByRole('button')).not.toBeInTheDocument();

    await u.click(within(line).getByRole('button', { name: '-c 32768' }));
    await u.click(within(screen.getByRole('note', { name: '-c' })).getByRole('button', { name: 'Change for this model…' }));
    const dialog = await screen.findByRole('dialog', { name: 'Load with options' });
    expect(within(dialog).getByRole('spinbutton', { name: 'Context' })).toHaveFocus();
  });

  /** An image model, its detail linking `vae`; `missing` names parts the library says are missing. */
  async function openImageSheet(missing: ('clip_l' | 't5xxl')[]) {
    transport.getModelDetail.mockResolvedValue({ id: 4, tags: [], components: [{ role: 'vae', path: '/models/ae.safetensors', present: true }] });
    explain.answer = launchExplanation(samplingExplanation(), { program: 'sd-server', args: [], narration: { model_name: 'flux', quantization: null, weights_bytes: 1, decisions: [] }, fit: null });
    renderInspector(guiModel({ id: 4, name: 'flux-schnell', imageFamily: 'flux1', missingComponents: missing }));
    await waitFor(() => expect(transport.getModelDetail).toHaveBeenCalled());
    const opened = await openSheet();
    const parts = await within(opened.dialog).findByRole('group', { name: 'Parts' });
    await waitFor(() => expect(parts).toHaveTextContent(/VAE ae\.safetensors/));
    return { ...opened, parts };
  }

  it('gives an image model its Parts line in place of the chat fields, a missing part named, and Load off', async () => {
    const { dialog, parts } = await openImageSheet(['clip_l']);

    expect(parts).toHaveTextContent(/▲ CLIP-L missing/i);
    expect(within(dialog).queryByRole('spinbutton', { name: 'Context' })).not.toBeInTheDocument();
    expect(within(dialog).queryByRole('checkbox', { name: /This load only/ })).not.toBeInTheDocument();
    // The runtime is there; the missing part alone keeps Load off.
    await within(parts).findByText(/Drawing with it is not available yet/);
    expect(within(dialog).getByRole('button', { name: 'Load' })).toBeDisabled();
  });

  it('keeps Load off while the image runtime alone is missing, installs it in place, and the inspector follows', async () => {
    sd.installed = false;
    const { u, dialog, parts } = await openImageSheet([]);
    expect(within(dialog).getByRole('button', { name: 'Load' })).toBeDisabled();

    await u.click(await within(parts).findByRole('button', { name: 'Install' }));
    expect(install.kinds).toEqual(['stable_diffusion']);
    sd.installed = true;
    act(() => install.onEvent.stable_diffusion({ type: 'completed', version: 'master-1' }));
    await waitFor(() => expect(within(dialog).getByRole('button', { name: 'Load' })).toBeEnabled());

    // The inspector under the sheet read the runtime again too.
    await u.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByText('Drawing needs stable-diffusion.cpp.')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Load' })).toBeEnabled();
  });

  it('loads an image model with every part and the runtime there, sending only its id', async () => {
    const { u, dialog } = await openImageSheet([]);
    await waitFor(() => expect(within(dialog).getByRole('button', { name: 'Load' })).toBeEnabled());
    await u.click(within(dialog).getByRole('button', { name: 'Load' }));

    await waitFor(() => expect(post).toHaveBeenCalledTimes(1));
    expect(bodies()).toEqual([['/api/servers/start', { id: 4, mlock: false }]]);
  });

  it('sends every sampling value the form offers (IN-6)', async () => {
    renderInspector();
    const { u, dialog } = await openSheet();
    const controls = [...dialog.querySelectorAll<HTMLInputElement>('input[id^="inference-param-"]')];
    const set: Record<string, unknown> = {};
    for (const input of controls) {
      const key = input.id.replace('inference-param-', '') as keyof typeof INFERENCE_PARAMS;
      fireEvent.change(input, { target: { value: String(INFERENCE_PARAMS[key].max) } });
      set[key] = INFERENCE_PARAMS[key].max;
    }
    await u.selectOptions(within(dialog).getByLabelText('Reasoning Effort'), 'high');
    set.reasoningEffort = 'high';
    await u.click(within(dialog).getByRole('checkbox', { name: /This load only/ }));
    await u.click(within(dialog).getByRole('button', { name: 'Load' }));

    await waitFor(() => expect(post).toHaveBeenCalledTimes(1));
    // Every `InferenceConfig` key but `seed`, which has no control.
    expect(Object.keys(set).sort()).toEqual(INFERENCE_CONFIG_KEYS.filter((k) => k !== 'seed').sort());
    expect(bodies()[0][1]).toEqual({ id: 9, mlock: false, inferenceParams: set });
  });

  it('shows an unset slider without a number, its caption naming what it inherits, and no intro', async () => {
    renderInspector();
    const { dialog } = await openSheet();
    const slider = within(dialog).getByLabelText('Frequency Penalty');

    expect(slider.parentElement).toHaveTextContent('—');
    expect(slider.parentElement).not.toHaveTextContent(/\d/);
    expect(within(dialog).queryByText(/Leave blank to inherit/)).not.toBeInTheDocument();
  });

  it('says nothing of the sampling sources while the explanation is read, and says so when it cannot be', async () => {
    explain.answer = new Promise(() => {});
    renderInspector(GEMMA, '/models/9?serve=1');
    const dialog = await screen.findByRole('dialog', { name: 'Load with options' });
    expect(within(dialog).queryByText('Resolution unavailable')).not.toBeInTheDocument();
    cleanup();
    cleanupModelEvents();

    explain.answer = null;
    renderInspector(GEMMA, '/models/9?serve=1');
    const failed = await screen.findByRole('dialog', { name: 'Load with options' });
    expect((await within(failed).findAllByText('Resolution unavailable')).length).toBeGreaterThan(0);
  });

  it('installs llama.cpp in place of Load when it is missing, then loads what was typed', async () => {
    setup.llamaInstalled = false;
    renderInspector();
    const { u, dialog } = await openSheet();
    await u.type(within(dialog).getByRole('spinbutton', { name: 'Context' }), '16384');
    await u.click(within(dialog).getByRole('checkbox', { name: /This load only/ }));
    await u.click(await within(dialog).findByRole('button', { name: 'Install llama.cpp' }));
    expect(install.kinds).toEqual(['llama']);
    expect(post).not.toHaveBeenCalled();

    setup.llamaInstalled = true;
    act(() => install.onEvent.llama({ type: 'completed', version: 'b1' }));
    await waitFor(() => expect(post).toHaveBeenCalledTimes(1));
    expect(bodies()[0][1]).toEqual({ id: 9, mlock: false, contextLength: 16384 });
  });

  it('offers Speculative when the plan drafts or the model has its own MTP default, without the tag', async () => {
    explain.answer = launchExplanation(samplingExplanation(), chatPlan({ args: [...chatPlan().args, ...MTP_ARGS] }));
    renderInspector();
    let { dialog } = await openSheet();
    expect(within(dialog).getByRole('group', { name: 'Speculative' })).toBeInTheDocument();
    cleanup();
    cleanupModelEvents();

    explain.answer = launchExplanation(samplingExplanation(), chatPlan());
    renderInspector(guiModel({ ...GEMMA, serverDefaults: { contextLength: null, mtp: false } }));
    ({ dialog } = await openSheet());
    expect(within(dialog).getByRole('group', { name: 'Speculative' })).toBeInTheDocument();
  });

  it('asks for a count when MTP is turned on for this load only and nothing else drafts', async () => {
    transport.getModelDetail.mockResolvedValue({ id: 9, tags: ['mtp'] });
    renderInspector(guiModel({ ...GEMMA, serverDefaults: { contextLength: null, mtp: false } }));
    await waitFor(() => expect(transport.getModelDetail).toHaveBeenCalled());
    const { u, dialog } = await openSheet();
    const spec = await within(dialog).findByRole('group', { name: 'Speculative' });
    await u.click(within(spec).getByRole('checkbox', { name: 'MTP' }));
    await u.click(within(dialog).getByRole('checkbox', { name: /This load only/ }));

    expect(within(dialog).getByText('Say how many draft tokens for this load.')).toBeInTheDocument();
    expect(within(dialog).getByRole('button', { name: 'Load' })).toBeDisabled();
    await u.type(within(spec).getByRole('spinbutton', { name: 'Draft tokens' }), '3');
    expect(within(dialog).getByRole('button', { name: 'Load' })).toBeEnabled();
  });
});
