/**
 * The endpoint's Load palette, driven through the real routes: it opens
 * from Load a model…, filters as typed, moves with ↑ ↓ Home End, loads on ⏎
 * (or a click) with the model's id and nothing typed, opens Load with
 * options on ⌥⏎, offers no model already loaded, says each row's fit and
 * unload from its explanation, reads those lazily (the highlighted row
 * first, then the six rows the scroll has reached, at most three at once
 * across opens, once per open, an answer to a closed open dropped), says
 * "fit unknown" for a failed read, keeps its highlight on a row as rows come
 * and go, offers the joined machine's models where they may load (and says
 * why not: away, not read, no Load), and closes on Esc with focus back on
 * its button, or on a click outside. The unload sentence is pinned to the CLI by
 * `contracts/unloadReason.test.ts`.
 *
 * The transport is a fake; each explain read waits until the test answers it.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

const library = vi.hoisted(() => ({ models: [] as unknown[] }));
const far = vi.hoisted(() => ({ group: null as unknown, fails: false }));
const transport = vi.hoisted(() => ({
  listModels: vi.fn(async () => library.models),
  listServers: vi.fn(async () => [] as unknown[]),
  getDownloadQueue: vi.fn(async () => null),
  listPairedModels: vi.fn(async () => {
    if (far.fails) throw new Error('tunnel dropped');
    return far.group;
  }),
  loadPairedModel: vi.fn(async (_id: number) => ({})),
  getModelDetail: vi.fn(async () => null),
  serveModel: vi.fn(async (_c: unknown) => ({ port: 9001 })),
  getProxyClients: vi.fn(async () => null),
  revealSecret: vi.fn(async () => null),
}));
/** The explain reads asked for, each waiting for the test to answer it; or `auto` answering at once. */
const explain = vi.hoisted(() => ({
  asked: [] as { id: number; answer: (v: unknown) => void; fail: (e: unknown) => void }[],
  reads: 0,
  auto: null as null | ((id: number) => unknown),
}));
const dashboard = vi.hoisted(() => ({ send: null as ((s: unknown) => void) | null }));

vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return {
    ...actual,
    getTransport: () => ({
      ...transport,
      getSettings: vi.fn(async () => ({ proxyAutostart: false })),
      subscribe: vi.fn(() => () => {}),
      onEventStreamOpen: vi.fn(() => () => {}),
    }),
  };
});
vi.mock('../../../src/services/transport/api/client', () => ({
  get: vi.fn((path: string) => {
    const read = /^\/api\/models\/(\d+)\/explain/.exec(path);
    if (!read) return Promise.resolve(null);
    const id = Number(read[1]);
    explain.reads += 1;
    if (explain.auto) return Promise.resolve(explain.auto(id));
    return new Promise((answer, fail) => explain.asked.push({ id, answer, fail }));
  }),
  post: vi.fn(async () => null),
}));
vi.mock('../../../src/services/transport/api/setup', () => ({
  getSetupStatus: vi.fn(async () => ({ llamaInstalled: true, llamaCanDownload: true })),
  getEngineStatus: vi.fn(async () => ({ installed: true })),
  streamEngineInstall: vi.fn(() => () => {}),
  getRecommendedModel: vi.fn(async () => null),
}));
vi.mock('../../../src/services/remoteEvents', () => ({ refreshRemoteStatus: vi.fn() }));
vi.mock('../../../src/services/clients/proxyDashboard', () => ({
  subscribeProxyDashboard: (_h: string, _p: number, _k: unknown, onSnapshot: (s: unknown) => void) => {
    dashboard.send = onSnapshot;
    return () => {
      dashboard.send = null;
    };
  },
  clearProxyCache: vi.fn(),
}));

import { cleanupModelEvents, initModelEvents } from '../../../src/services/modelEvents';
import { applyProxyStatus, resetProxyState } from '../../../src/services/proxyRegistry';
import { IDLE_STATUS, applyRemoteStatus, resetRemoteState } from '../../../src/services/remoteRegistry';
import type { ProxyStatus } from '../../../src/services/transport/types/proxy';
import type { RemoteStatus } from '../../../src/services/transport/types/remote';
import type { GgufModel } from '../../../src/types';
import type { LaunchPlan } from '../../../src/types/generated/LaunchPlan';
import { dashboardSnapshot, residentSlot } from '../fixtures/dashboard';
import { chatPlan, launchExplanation, samplingExplanation } from '../fixtures/explain';
import { FAR_FINGERPRINT, farEntry, pairedModels } from '../fixtures/fakeFarDaemon';
import { guiModel } from '../fixtures/model';
import { renderApp } from './harness';

const UP: ProxyStatus = { running: true, state: 'on', port: 8080, base_url: 'http://127.0.0.1:8080/v1', key: 'none_needed', pinned_model: null, pinned_to: null, pinned_model_id: null };
const DESK: RemoteStatus = {
  ...IDLE_STATUS,
  connected: { port: 41234, base_url: 'http://127.0.0.1:41234/v1', ticket_fingerprint: FAR_FINGERPRINT, path: 'direct', away_for_s: null },
  reach: 'connected',
  stored_ticket_fingerprint: FAR_FINGERPRINT,
  paired_name: 'desk',
};

const model = (id: number, name: string, extra: Partial<GgufModel> = {}) =>
  guiModel({ id, name, openaiId: name, quantization: 'Q4_K_M', sizeBytes: 5 * 1024 ** 3, ...extra });
const QWEN = model(1, 'Qwen3-4B');
const GEMMA = model(2, 'Gemma-3-4B-It', { quantization: 'Q8_0' });
const LLAMA = model(3, 'Llama-3.2-3B');
const FLUX = model(4, 'Flux-Schnell', { imageFamily: 'flux1', quantization: null });

const fitting = (plan: Partial<LaunchPlan> = {}) => launchExplanation(samplingExplanation(), chatPlan(plan));
const FIT_WORDS = 'Fits this machine with 32K context, of 128K trained.';
const UNLOAD_WORDS = 'Unloads Qwen3-30B-A3B first: it would not fit beside it.';

/** Play a dashboard frame in which the models `loaded` names are resident. */
const residents = (loaded: number[]) =>
  act(() =>
    dashboard.send?.(
      dashboardSnapshot({ admission: { ...dashboardSnapshot().admission, slots: loaded.map((id) => residentSlot(id, `Model ${id}`)) } }),
    ),
  );

async function renderPalette(models: GgufModel[], loaded: number[] = []) {
  library.models = models;
  act(() => applyProxyStatus(UP));
  initModelEvents();
  const view = { ...renderApp('/endpoint'), user: userEvent.setup() };
  await waitFor(() => expect(dashboard.send).not.toBeNull());
  residents(loaded);
  // The library has been read: the palette offers what it lists.
  await waitFor(() => expect(transport.listModels).toHaveBeenCalled());
  await act(async () => {});
  return view;
}

const button = () => screen.getByRole('button', { name: 'Load a model…' });
const palette = () => screen.getByRole('dialog', { name: 'Load a model' });
const field = () => within(palette()).getByRole('combobox', { name: 'Load a model' });
const options = () => within(palette()).queryAllByRole('option');
const option = (name: string) => within(palette()).getByRole('option', { name: new RegExp(name) });
const highlighted = () => document.getElementById(field().getAttribute('aria-activedescendant') ?? '');
const askedFor = () => explain.asked.map((a) => a.id);
/** Answer the oldest read still waiting for `id`. */
async function answer(id: number, value: unknown) {
  const at = explain.asked.findIndex((a) => a.id === id);
  const [read] = explain.asked.splice(at, 1);
  await act(async () => read.answer(value));
}

describe('the Load palette', () => {
  beforeEach(() => {
    for (const mock of Object.values(transport)) mock.mockClear();
    explain.asked = [];
    explain.auto = null;
    explain.reads = 0;
    far.group = null;
    far.fails = false;
    transport.listModels.mockImplementation(async () => library.models);
    transport.loadPairedModel.mockImplementation(async () => ({}));
    resetRemoteState();
  });
  afterEach(() => {
    act(() => resetProxyState());
    cleanupModelEvents();
  });

  it('opens from Load a model… with the filter focused, each row its name, quant and size; Esc closes and gives focus back', async () => {
    const { user } = await renderPalette([QWEN, GEMMA]);
    await user.click(button());

    expect(field()).toHaveFocus();
    expect(options()).toHaveLength(2);
    expect(option('Qwen3-4B')).toHaveTextContent('Q4_K_M · 5.00 GiB');
    expect(option('Gemma-3-4B-It')).toHaveTextContent('Q8_0 · 5.00 GiB');

    await user.keyboard('{Escape}');
    expect(screen.queryByRole('dialog', { name: 'Load a model' })).not.toBeInTheDocument();
    expect(button()).toHaveFocus();
  });

  it('filters by name and by quant as typed', async () => {
    const { user } = await renderPalette([QWEN, GEMMA, LLAMA]);
    await user.click(button());

    await user.keyboard('q8');
    expect(options().map((o) => o.id)).toEqual([option('Gemma-3-4B-It').id]);
    await user.clear(field());
    await user.keyboard('llama');
    expect(options()).toHaveLength(1);
    expect(option('Llama-3.2-3B')).toBeInTheDocument();
    await user.keyboard('zzz');
    expect(options()).toHaveLength(0);
    expect(within(palette()).getByText('No model matches.')).toBeInTheDocument();
  });

  it('↑ ↓ Home End move the highlight, named by the field', async () => {
    const { user } = await renderPalette([QWEN, GEMMA, LLAMA]);
    await user.click(button());

    expect(highlighted()).toBe(option('Qwen3-4B'));
    await user.keyboard('{ArrowDown}');
    expect(highlighted()).toBe(option('Gemma-3-4B-It'));
    expect(option('Gemma-3-4B-It')).toHaveAttribute('aria-selected', 'true');
    expect(option('Qwen3-4B')).toHaveAttribute('aria-selected', 'false');
    await user.keyboard('{End}');
    expect(highlighted()).toBe(option('Llama-3.2-3B'));
    await user.keyboard('{ArrowDown}');
    expect(highlighted()).toBe(option('Llama-3.2-3B'));
    await user.keyboard('{ArrowUp}');
    expect(highlighted()).toBe(option('Gemma-3-4B-It'));
    await user.keyboard('{Home}');
    expect(highlighted()).toBe(option('Qwen3-4B'));
  });

  it('⏎ loads the highlighted model with its id and nothing typed, and closes; a click does the same', async () => {
    const { user } = await renderPalette([QWEN, GEMMA]);
    await user.click(button());
    await user.keyboard('{ArrowDown}{Enter}');

    expect(transport.serveModel).toHaveBeenCalledTimes(1);
    expect(transport.serveModel.mock.calls[0][0]).toStrictEqual({ id: 2 });
    expect(screen.queryByRole('dialog', { name: 'Load a model' })).not.toBeInTheDocument();

    await user.click(button());
    await user.click(option('Qwen3-4B'));
    expect(transport.serveModel.mock.calls[1][0]).toStrictEqual({ id: 1 });
  });

  it("a failed load is one toast in the daemon's words", async () => {
    transport.serveModel.mockRejectedValueOnce(new Error('llama-server exited: out of memory'));
    const { user } = await renderPalette([QWEN]);
    await user.click(button());
    await user.keyboard('{Enter}');
    await waitFor(() => expect(screen.getByTestId('toasts')).toHaveTextContent("Didn't load: llama-server exited: out of memory"));
  });

  it('⌥⏎ opens Load with options for the highlighted model instead of loading it', async () => {
    explain.auto = () => fitting();
    const { user, at } = await renderPalette([QWEN, GEMMA]);
    await user.click(button());
    await user.keyboard('{ArrowDown}{Alt>}{Enter}{/Alt}');

    expect(transport.serveModel).not.toHaveBeenCalled();
    expect(await screen.findByRole('heading', { name: 'Load with options' })).toBeInTheDocument();
    expect(at()).toBe('/endpoint?inspect=model%3A2');
  });

  it('does not offer a model already loaded', async () => {
    const { user } = await renderPalette([QWEN, GEMMA], [1]);
    await user.click(button());
    expect(options()).toHaveLength(1);
    expect(option('Gemma-3-4B-It')).toBeInTheDocument();
  });

  it("says each row's fit from plan.fit, the CLI's unload sentence when the launch unloads first, and an image model's unload alone", async () => {
    const { user } = await renderPalette([QWEN, GEMMA, FLUX]);
    await user.click(button());
    await waitFor(() => expect(askedFor()).toEqual([1, 2, 4]));

    await answer(1, fitting());
    await answer(2, fitting({ unloads: { model: 'Qwen3-30B-A3B', reason: 'would_not_fit_beside' } }));
    await answer(4, fitting({ fit: null, unloads: { model: 'Flux-Dev', reason: 'one_image_model' } }));

    expect(option('Qwen3-4B')).toHaveTextContent(FIT_WORDS);
    expect(option('Qwen3-4B')).not.toHaveTextContent('Unloads');
    // The unload first, clear of the ellipsis, and the whole line on hover.
    expect(option('Gemma-3-4B-It')).toHaveTextContent(`${UNLOAD_WORDS} ${FIT_WORDS}`);
    expect(within(option('Gemma-3-4B-It')).getByTitle(`${UNLOAD_WORDS} ${FIT_WORDS}`)).toBeInTheDocument();
    expect(option('Flux-Schnell')).toHaveTextContent('Unloads Flux-Dev first: one image model is loaded at a time.');
    expect(option('Flux-Schnell')).not.toHaveTextContent('context');
  });

  it('a failed read, or a daemon without the route, says "fit unknown"', async () => {
    const { user } = await renderPalette([QWEN, GEMMA]);
    await user.click(button());
    await waitFor(() => expect(askedFor()).toEqual([1, 2]));

    const [first] = explain.asked.splice(0, 1);
    await act(async () => first.fail(new Error('daemon gone')));
    await answer(2, null);

    expect(option('Qwen3-4B')).toHaveTextContent('fit unknown');
    expect(option('Gemma-3-4B-It')).toHaveTextContent('fit unknown');
  });

  describe('the highlight', () => {
    it('typing puts it back on the first row', async () => {
      const { user } = await renderPalette([QWEN, GEMMA, LLAMA]);
      await user.click(button());
      await user.keyboard('{End}4b');
      expect(options()).toHaveLength(2);
      expect(highlighted()).toBe(option('Qwen3-4B'));
    });

    it('stays on its model when a row above it goes', async () => {
      const { user } = await renderPalette([QWEN, GEMMA, LLAMA]);
      await user.click(button());
      await user.keyboard('{ArrowDown}');
      residents([1]);
      expect(options()).toHaveLength(2);
      expect(highlighted()).toBe(option('Gemma-3-4B-It'));
    });

    it('when an app loads the highlighted model, the row now in its place takes it, and ⏎ acts on that row', async () => {
      const { user } = await renderPalette([QWEN, GEMMA, LLAMA]);
      await user.click(button());
      await user.keyboard('{End}');
      residents([3]);
      expect(options()).toHaveLength(2);
      expect(highlighted()).toBe(option('Gemma-3-4B-It'));

      await user.keyboard('{Enter}');
      expect(transport.serveModel.mock.calls[0][0]).toStrictEqual({ id: 2 });
    });
  });

  it('closes on a click outside it', async () => {
    const { user } = await renderPalette([QWEN]);
    await user.click(button());
    await user.click(screen.getByRole('heading', { name: 'Endpoint' }));
    expect(screen.queryByRole('dialog', { name: 'Load a model' })).not.toBeInTheDocument();
  });

  it("says nothing of an empty library before the library's first read answers", async () => {
    let answerList: (models: unknown[]) => void = () => {};
    transport.listModels.mockImplementation(() => new Promise((resolve) => (answerList = resolve)));
    const { user } = await renderPalette([]);
    await user.click(button());
    expect(within(palette()).queryByText('No models on this Mac.')).not.toBeInTheDocument();

    await act(async () => answerList([]));
    expect(await within(palette()).findByText('No models on this Mac.')).toBeInTheDocument();
  });

  describe('reads', () => {
    const MANY = Array.from({ length: 8 }, (_, i) => model(i + 1, `Model-${String.fromCharCode(65 + i)}`));

    it('reads at most three at once, and the highlighted row before the rest in view', async () => {
      const { user } = await renderPalette(MANY);
      await user.click(button());
      await waitFor(() => expect(askedFor()).toEqual([1, 2, 3]));

      await user.keyboard('{End}');
      expect(askedFor()).toEqual([1, 2, 3]);
      await answer(1, fitting());
      await waitFor(() => expect(askedFor()).toEqual([2, 3, 8]));
      await answer(2, fitting());
      await waitFor(() => expect(askedFor()).toEqual([3, 8, 4]));
    });

    it('reads a row once per open: highlighting it again asks nothing', async () => {
      const { user } = await renderPalette([QWEN, GEMMA]);
      await user.click(button());
      await waitFor(() => expect(askedFor()).toEqual([1, 2]));
      await answer(1, fitting());
      await answer(2, fitting());

      await user.keyboard('{ArrowDown}{ArrowUp}{ArrowDown}');
      expect(explain.reads).toBe(2);
      expect(option('Qwen3-4B')).toHaveTextContent(FIT_WORDS);
    });

    it('reads the six rows the scroll has reached, after the highlighted row, and none past them', async () => {
      const TWELVE = Array.from({ length: 12 }, (_, i) => model(i + 1, `Row-${String.fromCharCode(65 + i)}`));
      const { user } = await renderPalette(TWELVE);
      await user.click(button());
      await waitFor(() => expect(askedFor()).toEqual([1, 2, 3]));

      const list = within(palette()).getByRole('listbox', { name: 'Models to load' });
      list.scrollTop = 52 * 4;
      fireEvent.scroll(list);
      for (const id of [1, 2, 3]) await answer(id, fitting());
      await waitFor(() => expect(askedFor()).toEqual([5, 6, 7]));
      for (const id of [5, 6, 7]) await answer(id, fitting());
      await waitFor(() => expect(askedFor()).toEqual([8, 9, 10]));
      for (const id of [8, 9, 10]) await answer(id, fitting());
      await act(async () => {});
      expect(askedFor()).toEqual([]);
      expect(explain.reads).toBe(9);
    });

    it('counts the reads a closed open left out: a quick close and reopen keeps three in flight', async () => {
      const { user } = await renderPalette(MANY);
      await user.click(button());
      await waitFor(() => expect(askedFor()).toEqual([1, 2, 3]));
      await user.keyboard('{Escape}');
      await user.click(button());
      await act(async () => {});
      expect(askedFor()).toEqual([1, 2, 3]);

      await answer(1, fitting({ unloads: { model: 'Qwen3-30B-A3B', reason: 'would_not_fit_beside' } }));
      await waitFor(() => expect(askedFor()).toEqual([2, 3, 1]));
      expect(option('Model-A')).not.toHaveTextContent('Unloads');
    });

    it('drops an answer that lands after its palette closed, and reads again in the next open', async () => {
      const { user } = await renderPalette([QWEN]);
      await user.click(button());
      await waitFor(() => expect(askedFor()).toEqual([1]));
      await user.keyboard('{Escape}');

      await user.click(button());
      await waitFor(() => expect(askedFor()).toEqual([1, 1]));
      await answer(1, fitting({ unloads: { model: 'Qwen3-30B-A3B', reason: 'would_not_fit_beside' } }));
      expect(option('Qwen3-4B')).not.toHaveTextContent('Unloads');

      await answer(1, fitting());
      expect(option('Qwen3-4B')).toHaveTextContent(FIT_WORDS);
    });
  });

  describe('On <machine>', () => {
    const joined = (actions: string[] = ['list', 'detail', 'chat', 'load']) => {
      far.group = { ...pairedModels([farEntry('qwen3-32b', 7, { quantization: 'Q4_K_M' })]), actions };
      act(() => applyRemoteStatus(DESK));
    };

    it('is not drawn while no machine is joined', async () => {
      const { user } = await renderPalette([QWEN]);
      await user.click(button());
      expect(within(palette()).queryByRole('group')).not.toBeInTheDocument();
      expect(transport.listPairedModels).not.toHaveBeenCalled();
    });

    it("lists the joined machine's models after this Mac's, and ⏎ loads one there", async () => {
      joined();
      const { user } = await renderPalette([QWEN]);
      await waitFor(() => expect(transport.listPairedModels).toHaveBeenCalled());
      await user.click(button());
      const group = await within(palette()).findByRole('group', { name: 'On desk' });
      expect(within(group).getByRole('option', { name: /qwen3-32b/ })).toHaveTextContent('Load on desk');

      await user.keyboard('{End}{Enter}');
      await waitFor(() => expect(transport.loadPairedModel).toHaveBeenCalledWith(7));
      expect(transport.serveModel).not.toHaveBeenCalled();
    });

    it("where that machine is away, its rows say so and neither ⏎ nor ⌥⏎ loads", async () => {
      joined();
      const { user } = await renderPalette([QWEN]);
      await waitFor(() => expect(transport.listPairedModels).toHaveBeenCalled());
      act(() => applyRemoteStatus({ ...DESK, reach: 'away', connected: { ...DESK.connected!, away_for_s: 30 } }));
      await user.click(button());
      const row = await within(palette()).findByRole('option', { name: /qwen3-32b/ });
      expect(row).toHaveAttribute('aria-disabled', 'true');
      expect(row).toHaveTextContent('desk is away; Load waits until it is back.');

      await user.keyboard('{End}{Enter}{Alt>}{Enter}{/Alt}');
      expect(transport.loadPairedModel).not.toHaveBeenCalled();
    });

    it('⌥⏎ on a row that may load there does nothing: there is no Load with options for it', async () => {
      joined();
      const { user } = await renderPalette([QWEN]);
      await waitFor(() => expect(transport.listPairedModels).toHaveBeenCalled());
      await user.click(button());
      await within(palette()).findByRole('group', { name: 'On desk' });
      await user.keyboard('{End}{Alt>}{Enter}{/Alt}');
      expect(transport.loadPairedModel).not.toHaveBeenCalled();
      expect(transport.serveModel).not.toHaveBeenCalled();
    });

    it("after a load there it reads that machine's rows again; a failed re-read says they could not be read", async () => {
      joined();
      const { user } = await renderPalette([QWEN]);
      await waitFor(() => expect(transport.listPairedModels).toHaveBeenCalled());
      const before = transport.listPairedModels.mock.calls.length;
      far.fails = true;
      await user.click(button());
      await within(palette()).findByRole('group', { name: 'On desk' });
      await user.keyboard('{End}{Enter}');
      await waitFor(() => expect(transport.listPairedModels.mock.calls.length).toBeGreaterThan(before));

      await user.click(button());
      const row = await within(palette()).findByRole('option', { name: /qwen3-32b/ });
      expect(row).toHaveTextContent("desk's models could not be read just now.");
      expect(row).toHaveAttribute('aria-disabled', 'true');
    });

    it("a failed load there is one toast in that machine's words", async () => {
      joined();
      transport.loadPairedModel.mockRejectedValueOnce(new Error('desk refused: out of memory'));
      const { user } = await renderPalette([QWEN]);
      await waitFor(() => expect(transport.listPairedModels).toHaveBeenCalled());
      await user.click(button());
      await within(palette()).findByRole('group', { name: 'On desk' });
      await user.keyboard('{End}{Enter}');
      await waitFor(() => expect(screen.getByTestId('toasts')).toHaveTextContent("Didn't load: desk refused: out of memory"));
    });

    it("with every model here loaded, says so above the joined machine's rows", async () => {
      joined();
      const { user } = await renderPalette([QWEN], [1]);
      await waitFor(() => expect(transport.listPairedModels).toHaveBeenCalled());
      await user.click(button());
      await within(palette()).findByRole('group', { name: 'On desk' });
      expect(within(palette()).getByText('Every model on this Mac is loaded.')).toBeInTheDocument();
    });

    it('where that machine does not offer Load, its rows say so and ⏎ does nothing', async () => {
      joined(['list', 'detail', 'chat']);
      const { user } = await renderPalette([QWEN]);
      await waitFor(() => expect(transport.listPairedModels).toHaveBeenCalled());
      await user.click(button());
      const row = await within(palette()).findByRole('option', { name: /qwen3-32b/ });
      expect(row).toHaveAttribute('aria-disabled', 'true');
      expect(row).toHaveTextContent('desk does not offer Load.');

      await user.keyboard('{End}{Enter}');
      expect(transport.loadPairedModel).not.toHaveBeenCalled();
      expect(palette()).toBeInTheDocument();
    });
  });
});
