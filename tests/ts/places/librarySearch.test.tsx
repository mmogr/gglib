/**
 * The library's one search, through the real routes and registries: typing
 * filters the library and asks Hugging Face nothing; Enter, or the hint row,
 * searches the Hub, with the kind, the sort and "More results" each asking
 * again; an overtaken answer is dropped; `user/repo` opens the repo in the
 * inspector and `user/repo:QUANT` offers its download; the empty field
 * holds the suggestion for this machine; `+ ▾` and the menu's addresses
 * focus the search or add a GGUF file.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

import type { HfModelSummary, HfSearchRequest, HfSearchResponse } from '../../../src/types';

const library = vi.hoisted(() => ({ models: [] as unknown[] }));
const suggestion = vi.hoisted(() => ({ answer: null as unknown }));
const transport = vi.hoisted(() => ({
  listModels: vi.fn(async () => library.models),
  listServers: vi.fn(async () => []),
  getDownloadQueue: vi.fn(async () => ({ revision: 1, waiting: [], finished: [], max_size: 10, full: false })),
  listPairedModels: vi.fn(async () => null),
  browseHfModels: vi.fn(async (_r: HfSearchRequest): Promise<HfSearchResponse> => ({ models: [], next_cursor: null, total_count: null })),
  getHfModelSummary: vi.fn(async (_id: string): Promise<HfModelSummary> => hfRepo('owner/x')),
  getHfToolSupport: vi.fn(async (_id: string) => ({ supports_tool_calls: false, confidence: 0, detected_format: null })),
  getHfQuantizations: vi.fn(async () => new Promise(() => {})),
  queueDownload: vi.fn(async (_p: { modelId: string; quantization?: string }) => ({ id: 'q-1' })),
  addModel: vi.fn(async (_p: { filePath: string }) => ({})),
}));

vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return {
    ...actual,
    getTransport: () => ({
      ...transport,
      getModelDetail: vi.fn(async () => null),
      getSettings: vi.fn(async () => ({})),
      subscribe: vi.fn(() => () => {}),
      onEventStreamOpen: vi.fn(() => () => {}),
    }),
  };
});
vi.mock('../../../src/services/transport/api/client', () => ({ get: vi.fn(async () => null) }));
vi.mock('../../../src/services/transport/api/setup', () => ({
  getSetupStatus: vi.fn(async () => ({ llamaInstalled: true, llamaCanDownload: true })),
  streamEngineInstall: vi.fn(() => () => {}),
  getRecommendedModel: vi.fn(async () => suggestion.answer),
}));
vi.mock('../../../src/services/remoteEvents', () => ({ refreshRemoteStatus: vi.fn() }));
// The desktop app's native picker, when a test makes this window the desktop's.
const platform = vi.hoisted(() => ({
  desktop: false,
  pick: vi.fn(async (): Promise<{ path: string | null; cancelled: boolean }> => ({ path: null, cancelled: true })),
}));
vi.mock('../../../src/services/platform', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/platform');
  return { ...actual, isDesktop: () => platform.desktop, pickGgufFile: platform.pick };
});

import { cleanupModelEvents, initModelEvents } from '../../../src/services/modelEvents';
import { cleanupDownloadEvents, initDownloadEvents } from '../../../src/services/downloadsEvents';
import { enterAction } from '../../../src/places/library/hf/useLibrarySearch';
import { renderApp } from './harness';
import { guiModel } from '../fixtures/model';

function hfRepo(id: string, extra: Partial<HfModelSummary> = {}): HfModelSummary {
  return { id, name: id.split('/')[1], author: id.split('/')[0], downloads: 0, likes: 0, last_modified: null, parameters_b: null, description: null, tags: [], ...extra };
}

/** A page of `ids`, with `next` as the cursor after it. */
const page = (ids: string[], next: string | null = null): HfSearchResponse => ({ models: ids.map((id) => hfRepo(id)), next_cursor: next, total_count: null });

/** The requests the Hub was sent, in order. */
const asked = () => transport.browseHfModels.mock.calls.map(([r]) => r);

/** The names of the Hugging Face group's rows. */
const hfRows = () =>
  within(screen.getByRole('listbox', { name: 'Hugging Face' }))
    .getAllByRole('option')
    .map((o) => o.firstElementChild?.firstElementChild?.textContent);

function renderLibrary(path = '/library') {
  initModelEvents();
  initDownloadEvents();
  return renderApp(path);
}

const field = () => screen.getByRole('searchbox', { name: 'Search your models, or Hugging Face' });

describe('the one search', () => {
  beforeEach(() => {
    library.models = [guiModel({ id: 1, name: 'Qwen3-4B' })];
    suggestion.answer = null;
    for (const mock of Object.values(transport)) mock.mockClear();
    transport.browseHfModels.mockImplementation(async () => page([]));
    transport.getHfModelSummary.mockImplementation(async (id) => hfRepo(id));
    transport.addModel.mockImplementation(async () => ({}));
    platform.desktop = false;
    platform.pick.mockClear();
  });
  afterEach(() => {
    cleanupModelEvents();
    cleanupDownloadEvents();
  });

  it('filters the library as it is typed, asks Hugging Face nothing, and offers the hint', async () => {
    renderLibrary();
    await screen.findByRole('option', { name: /Qwen3-4B/ });
    await userEvent.setup().type(field(), 'mistral');

    expect(screen.queryByRole('option', { name: /Qwen3-4B/ })).not.toBeInTheDocument();
    expect(screen.getByText('Nothing in your library matches. Press ↵ to search Hugging Face.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: "Search Hugging Face for 'mistral' ↵" })).toBeInTheDocument();
    expect(transport.browseHfModels).not.toHaveBeenCalled();
    expect(screen.queryByRole('region', { name: 'Hugging Face' })).not.toBeInTheDocument();
  });

  it('searches the Hub on Enter, and draws each repo with its params, tool calling and downloads', async () => {
    transport.browseHfModels.mockResolvedValue({ ...page([]), models: [hfRepo('owner/Qwen3-8B-GGUF', { parameters_b: 8, downloads: 1_200_000 })] });
    transport.getHfToolSupport.mockResolvedValue({ supports_tool_calls: true, confidence: 1, detected_format: null });
    renderLibrary();
    await userEvent.setup().type(field(), 'qwen{Enter}');

    expect(asked()).toEqual([
      { query: 'qwen', min_params_b: null, max_params_b: null, cursor: null, limit: 100, sort_by: 'downloads', sort_ascending: false, kind: 'chat' },
    ]);
    const row = await screen.findByRole('option', { name: /owner\/Qwen3-8B-GGUF/ });
    await waitFor(() => expect(row).toHaveTextContent('8.0B · calls tools · 1.2M downloads'));
  });

  it('searches from the hint row as Enter does', async () => {
    transport.browseHfModels.mockResolvedValue(page(['owner/a']));
    renderLibrary();
    const user = userEvent.setup();
    await user.type(field(), 'gemma');
    await user.click(screen.getByRole('button', { name: "Search Hugging Face for 'gemma' ↵" }));

    expect(asked().map((r) => r.query)).toEqual(['gemma']);
    expect(await screen.findByRole('option', { name: /owner\/a/ })).toBeInTheDocument();
  });

  it('asks again with the kind and the sort picked', async () => {
    renderLibrary();
    const user = userEvent.setup();
    await user.type(field(), 'flux{Enter}');
    const group = await screen.findByRole('region', { name: 'Hugging Face' });

    await user.click(within(group).getByRole('button', { name: 'Image' }));
    await waitFor(() => expect(asked()).toHaveLength(2));
    expect(asked()[1]).toMatchObject({ query: 'flux', kind: 'image', cursor: null });

    // Each sort starts in its own direction: Alphabetical ascending, after Downloads descending.
    await user.click(within(group).getByRole('button', { name: 'Sort Hugging Face' }));
    await user.click(screen.getByRole('menuitemradio', { name: 'Alphabetical' }));
    await waitFor(() => expect(asked()).toHaveLength(3));
    expect(asked()[2]).toMatchObject({ kind: 'image', sort_by: 'id', sort_ascending: true });

    await user.click(within(group).getByRole('button', { name: 'Sort Hugging Face' }));
    await user.click(screen.getByRole('menuitemradio', { name: 'Descending' }));
    await waitFor(() => expect(asked()).toHaveLength(4));
    expect(asked()[3]).toMatchObject({ sort_by: 'id', sort_ascending: false });
  });

  it('carries on from the last page\'s cursor with More results, until the Hub has no more', async () => {
    transport.browseHfModels.mockImplementation(async (r) => (r.cursor === null ? page(['owner/a'], 'c1') : page(['owner/b'])));
    renderLibrary();
    const user = userEvent.setup();
    await user.type(field(), 'llama{Enter}');
    await screen.findByRole('option', { name: /owner\/a/ });

    await user.click(screen.getByRole('button', { name: 'More results' }));
    await screen.findByRole('option', { name: /owner\/b/ });
    expect(asked()[1]).toMatchObject({ query: 'llama', cursor: 'c1' });
    expect(hfRows()).toEqual(['owner/a', 'owner/b']);
    expect(screen.queryByRole('button', { name: 'More results' })).not.toBeInTheDocument();
  });

  it('drops an answer a newer search has overtaken', async () => {
    let answerFirst: (r: HfSearchResponse) => void = () => {};
    transport.browseHfModels
      .mockImplementationOnce(() => new Promise((resolve) => (answerFirst = resolve)))
      .mockImplementationOnce(async () => page(['owner/second']));
    renderLibrary();
    const user = userEvent.setup();
    await user.type(field(), 'first{Enter}');
    await user.clear(field());
    await user.type(field(), 'second{Enter}');
    await screen.findByRole('option', { name: /owner\/second/ });

    answerFirst(page(['owner/first']));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(hfRows()).toEqual(['owner/second']);
  });

  it('says when nothing on the Hub matches, and why a search failed', async () => {
    renderLibrary();
    const user = userEvent.setup();
    await user.type(field(), 'zzz{Enter}');
    expect(await screen.findByText("No GGUF repos match 'zzz'.")).toBeInTheDocument();

    transport.browseHfModels.mockRejectedValueOnce(new Error('Hugging Face rate limit reached'));
    await user.type(field(), '{Enter}');
    expect(await screen.findByText('Hugging Face rate limit reached')).toBeInTheDocument();
  });

  it('opens an exact user/repo in the inspector at its address, read once, and a pick of the library drops it', async () => {
    transport.getHfModelSummary.mockResolvedValue(hfRepo('owner/Qwen3-8B-GGUF'));
    const { at } = renderLibrary();
    const user = userEvent.setup();
    await user.type(field(), 'owner/Qwen3-8B-GGUF{Enter}');

    const inspector = await screen.findByRole('complementary', { name: 'Inspector' });
    expect(within(inspector).getByRole('heading', { name: 'owner/Qwen3-8B-GGUF' })).toBeInTheDocument();
    expect(at()).toBe('/get/owner/Qwen3-8B-GGUF');
    // The inspector takes the summary Enter read; it does not read it again.
    expect(transport.getHfModelSummary).toHaveBeenCalledTimes(1);
    expect(transport.getHfModelSummary).toHaveBeenCalledWith('owner/Qwen3-8B-GGUF');
    expect(transport.browseHfModels).not.toHaveBeenCalled();
    expect(field()).toHaveValue('');

    await user.click(screen.getByRole('option', { name: /Qwen3-4B/ }));
    expect(at()).toBe('/models/1');
    expect(screen.queryByRole('heading', { name: 'owner/Qwen3-8B-GGUF' })).not.toBeInTheDocument();
  });

  it('a Hugging Face pick leaves a model picked in the library', async () => {
    transport.browseHfModels.mockResolvedValue(page(['owner/a']));
    const { at } = renderLibrary('/models/1');
    const user = userEvent.setup();
    await user.type(field(), 'a{Enter}');
    await user.click(await screen.findByRole('option', { name: /owner\/a/ }));

    expect(at()).toBe('/get/owner/a');
    expect(screen.getByRole('option', { name: /owner\/a/ })).toHaveAttribute('aria-selected', 'true');
    expect(within(screen.getByRole('complementary', { name: 'Inspector' })).getByRole('heading', { name: 'owner/a' })).toBeInTheDocument();
  });

  it('the repos are one Tab stop, and ↓ opens the next in the inspector', async () => {
    transport.browseHfModels.mockResolvedValue(page(['owner/a', 'owner/b']));
    const { at } = renderLibrary();
    const user = userEvent.setup();
    await user.type(field(), 'x{Enter}');
    const a = await screen.findByRole('option', { name: /owner\/a/ });
    const b = screen.getByRole('option', { name: /owner\/b/ });
    expect([a.tabIndex, b.tabIndex]).toEqual([0, -1]);

    a.focus();
    await user.keyboard('{ArrowDown}');
    expect(document.activeElement).toBe(b);
    await waitFor(() => expect(at()).toBe('/get/owner/b'));
    expect([a.tabIndex, b.tabIndex]).toEqual([-1, 0]);
  });

  it('a run of keys opens the repo it ends on, read once, as one history entry', async () => {
    transport.browseHfModels.mockResolvedValue(page(['owner/a', 'owner/b', 'owner/c']));
    const { at, router } = renderLibrary();
    const user = userEvent.setup();
    await user.type(field(), 'x{Enter}');
    await user.click(await screen.findByRole('option', { name: /owner\/a/ }));
    expect(at()).toBe('/get/owner/a');
    transport.getHfQuantizations.mockClear();

    await user.keyboard('{ArrowDown}{ArrowDown}');
    expect(document.activeElement).toBe(screen.getByRole('option', { name: /owner\/c/ }));
    // The address waits for the run to end.
    expect(at()).toBe('/get/owner/a');
    await waitFor(() => expect(at()).toBe('/get/owner/c'));
    expect(transport.getHfQuantizations.mock.calls.map((c) => (c as unknown[])[0])).toEqual(['owner/c']);

    await act(() => router.navigate(-1));
    expect(at()).toBe('/library');
  });

  it("offers user/repo:QUANT's download as the first row, and queues it", async () => {
    renderLibrary();
    const user = userEvent.setup();
    await user.type(field(), 'owner/zeta-GGUF:Q4_K_M{Enter}');

    const group = await screen.findByRole('region', { name: 'Hugging Face' });
    expect(within(group).getByText('Download Q4_K_M from owner/zeta-GGUF')).toBeInTheDocument();
    await user.click(within(group).getByRole('button', { name: 'Download' }));
    await waitFor(() => expect(transport.queueDownload).toHaveBeenCalledWith({ modelId: 'owner/zeta-GGUF', quantization: 'Q4_K_M' }));
    expect(transport.browseHfModels).not.toHaveBeenCalled();
    await waitFor(() => expect(field()).toHaveValue(''));
  });

  it("holds the machine's suggestion with the empty field focused, and queues it", async () => {
    suggestion.answer = {
      repo: 'owner/small-GGUF',
      rationale: 'the smallest model in the list that still calls tools',
      requiredBytes: 5 * 2 ** 30,
      budgetBytes: 16 * 2 ** 30,
      budgetSource: 'unifiedMemory',
      budgetLabel: 'unified memory',
      headroomBytes: 11 * 2 ** 30,
      context: 32768,
    };
    renderLibrary();
    await screen.findByRole('option', { name: /Qwen3-4B/ });
    await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
    expect(screen.queryByRole('region', { name: 'Hugging Face' })).not.toBeInTheDocument();
    await userEvent.setup().click(field());

    const suggested = await screen.findByRole('region', { name: 'Suggested for this Mac' });
    expect(suggested).toHaveTextContent('owner/small-GGUF');
    expect(suggested).toHaveTextContent('The smallest model in the list that still calls tools. Needs 5.0 GiB of 16.0 GiB unified memory.');
    await userEvent.setup().click(within(suggested).getByRole('button', { name: 'Download' }));
    await waitFor(() => expect(transport.queueDownload).toHaveBeenCalledWith({ modelId: 'owner/small-GGUF', quantization: undefined }));
  });

  it('says so when nothing gglib would suggest fits', async () => {
    renderLibrary();
    await userEvent.setup().click(field());
    expect(await screen.findByText('Nothing gglib would suggest fits. Search for a smaller model below.')).toBeInTheDocument();
  });

  it('Find on Hugging Face focuses the search in its scope, by the address the menu uses too', async () => {
    const { at } = renderLibrary();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Add' }));
    await user.click(screen.getByRole('menuitem', { name: 'Find on Hugging Face' }));

    await waitFor(() => expect(at()).toBe('/library'));
    expect(field()).toHaveFocus();
    expect(await screen.findByRole('region', { name: 'Hugging Face' })).toBeInTheDocument();
  });

  it('Add a GGUF file… on the web takes a typed path, and says why the daemon refused one', async () => {
    const { at } = renderLibrary();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Add' }));
    await user.click(screen.getByRole('menuitem', { name: 'Add a GGUF file…' }));
    await waitFor(() => expect(at()).toBe('/library'));

    const form = screen.getByRole('form', { name: 'Add a GGUF file' });
    transport.addModel.mockRejectedValueOnce(new Error('Not a GGUF file: /tmp/x.bin'));
    await user.type(within(form).getByRole('textbox', { name: 'Path to a GGUF file' }), '/tmp/x.bin');
    await user.click(within(form).getByRole('button', { name: 'Add' }));
    expect(await within(form).findByText('Not a GGUF file: /tmp/x.bin')).toBeInTheDocument();

    await user.clear(within(form).getByRole('textbox', { name: 'Path to a GGUF file' }));
    await user.type(within(form).getByRole('textbox', { name: 'Path to a GGUF file' }), '/models/a.gguf{Enter}');
    await waitFor(() => expect(transport.addModel).toHaveBeenLastCalledWith({ filePath: '/models/a.gguf' }));
    await waitFor(() => expect(screen.queryByRole('form', { name: 'Add a GGUF file' })).not.toBeInTheDocument());
  });

  it('Add a GGUF file… on the desktop adds what the native picker chose, and keeps the path only to say why it failed', async () => {
    platform.desktop = true;
    platform.pick.mockResolvedValue({ path: '/models/b.gguf', cancelled: false });
    renderLibrary('/library?add=file');
    await waitFor(() => expect(transport.addModel).toHaveBeenCalledWith({ filePath: '/models/b.gguf' }));
    await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
    expect(screen.queryByRole('form', { name: 'Add a GGUF file' })).not.toBeInTheDocument();
  });

  it('a desktop add the daemon refuses opens the row with the picked path and the reason', async () => {
    platform.desktop = true;
    platform.pick.mockResolvedValue({ path: '/tmp/x.bin', cancelled: false });
    transport.addModel.mockRejectedValueOnce(new Error('Not a GGUF file: /tmp/x.bin'));
    renderLibrary('/library?add=file');

    const form = await screen.findByRole('form', { name: 'Add a GGUF file' });
    expect(within(form).getByRole('textbox', { name: 'Path to a GGUF file' })).toHaveValue('/tmp/x.bin');
    expect(within(form).getByText('Not a GGUF file: /tmp/x.bin')).toBeInTheDocument();
  });

  it("opens the menu's Add Model File address as the typed path", async () => {
    renderLibrary('/library?add=file');
    expect(await screen.findByRole('form', { name: 'Add a GGUF file' })).toBeInTheDocument();
  });

  it("drops a repo's late answer once the field is edited and a library model picked", async () => {
    let answer: (m: HfModelSummary) => void = () => {};
    transport.getHfModelSummary.mockImplementationOnce(() => new Promise((resolve) => (answer = resolve)));
    const { at } = renderLibrary();
    const user = userEvent.setup();
    await user.type(field(), 'owner/slow-GGUF{Enter}');
    await user.clear(field());
    await user.type(field(), 'qwen');
    await user.click(screen.getByRole('option', { name: /Qwen3-4B/ }));
    expect(at()).toBe('/models/1');

    await act(async () => answer(hfRepo('owner/slow-GGUF')));
    expect(screen.queryByRole('heading', { name: 'owner/slow-GGUF' })).not.toBeInTheDocument();
    expect(field()).toHaveValue('qwen');
    const rows = within(screen.getByRole('region', { name: 'Library' }));
    expect(rows.getAllByRole('option', { selected: true }).map((o) => o.textContent)).toEqual([expect.stringContaining('Qwen3-4B')]);
  });

  it("drops an open's answer a later Enter has overtaken", async () => {
    let answerFirst: (m: HfModelSummary) => void = () => {};
    transport.getHfModelSummary
      .mockImplementationOnce(() => new Promise((resolve) => (answerFirst = resolve)))
      .mockImplementationOnce(async () => hfRepo('owner/b-GGUF'));
    renderLibrary();
    const user = userEvent.setup();
    await user.type(field(), 'owner/a-GGUF{Enter}');
    await user.clear(field());
    await user.type(field(), 'owner/b-GGUF{Enter}');
    const inspector = await screen.findByRole('complementary', { name: 'Inspector' });
    expect(within(inspector).getByRole('heading', { name: 'owner/b-GGUF' })).toBeInTheDocument();

    await act(async () => answerFirst(hfRepo('owner/a-GGUF')));
    expect(within(inspector).getByRole('heading', { name: 'owner/b-GGUF' })).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'owner/a-GGUF' })).not.toBeInTheDocument();
  });

  /** Enter `repo`, whose answer waits for the returned function. */
  async function enterSlowRepo(repo: string) {
    let answer: (m: HfModelSummary) => void = () => {};
    transport.getHfModelSummary.mockImplementationOnce(() => new Promise((resolve) => (answer = resolve)));
    await userEvent.setup().type(field(), `${repo}{Enter}`);
    return (m: HfModelSummary) => act(async () => answer(m));
  }

  it("drops a repo's late answer once the field is edited", async () => {
    renderLibrary();
    const answer = await enterSlowRepo('owner/slow-GGUF');
    await userEvent.setup().type(field(), '-v2');

    await answer(hfRepo('owner/slow-GGUF'));
    expect(screen.queryByRole('complementary', { name: 'Inspector' })).not.toBeInTheDocument();
    expect(field()).toHaveValue('owner/slow-GGUF-v2');
  });

  it("drops a repo's late answer once the address picks a library model", async () => {
    const { router, at } = renderLibrary();
    await screen.findByRole('option', { name: /Qwen3-4B/ });
    const answer = await enterSlowRepo('owner/slow-GGUF');
    await act(() => router.navigate('/models/1'));

    await answer(hfRepo('owner/slow-GGUF'));
    expect(at()).toBe('/models/1');
    expect(screen.queryByRole('heading', { name: 'owner/slow-GGUF' })).not.toBeInTheDocument();
    expect(field()).toHaveValue('owner/slow-GGUF');
  });

  it("keeps a second Enter's refusal over the first Enter's late answer", async () => {
    renderLibrary();
    const answer = await enterSlowRepo('owner/slow-GGUF');
    transport.getHfModelSummary.mockRejectedValueOnce(new Error('rate limited'));
    await userEvent.setup().type(field(), '{Enter}');
    expect(await screen.findByText('rate limited')).toBeInTheDocument();

    await answer(hfRepo('owner/slow-GGUF'));
    expect(screen.queryByRole('complementary', { name: 'Inspector' })).not.toBeInTheDocument();
    expect(screen.getByText('rate limited')).toBeInTheDocument();
    expect(field()).toHaveValue('owner/slow-GGUF');
  });

  it("drops a first Enter's late refusal while a second Enter is still opening", async () => {
    renderLibrary();
    let refuse: (e: Error) => void = () => {};
    transport.getHfModelSummary.mockImplementationOnce(() => new Promise((_, reject) => (refuse = reject)));
    const user = userEvent.setup();
    await user.type(field(), 'owner/slow-GGUF{Enter}');
    transport.getHfModelSummary.mockImplementationOnce(() => new Promise(() => {}));
    await user.type(field(), '{Enter}');

    await act(async () => refuse(new Error('rate limited')));
    expect(screen.queryByText('rate limited')).not.toBeInTheDocument();
  });

  it("drops a repo's late answer once the library is left", async () => {
    const { router, at } = renderLibrary('/models/1');
    await screen.findByRole('option', { name: /Qwen3-4B/ });
    const answer = await enterSlowRepo('owner/slow-GGUF');
    await act(() => router.navigate('/diagnostics/benchmarks'));

    await answer(hfRepo('owner/slow-GGUF'));
    expect(at()).toBe('/diagnostics/benchmarks');
  });

  it('says why a named repo could not be opened, and keeps the field', async () => {
    transport.getHfModelSummary.mockRejectedValueOnce(new Error('Repository owner/gone not found'));
    renderLibrary();
    await userEvent.setup().type(field(), 'owner/gone{Enter}');

    const group = await screen.findByRole('region', { name: 'Hugging Face' });
    expect(await within(group).findByText('Repository owner/gone not found')).toBeInTheDocument();
    expect(field()).toHaveValue('owner/gone');
    expect(screen.queryByRole('complementary', { name: 'Inspector' })).not.toBeInTheDocument();
  });

  it('takes the results and More results away once the field says something else', async () => {
    transport.browseHfModels.mockImplementation(async () => page(['owner/a'], 'c1'));
    renderLibrary();
    const user = userEvent.setup();
    await user.type(field(), 'llama{Enter}');
    await screen.findByRole('option', { name: /owner\/a/ });

    await user.type(field(), 'x');
    expect(screen.queryByRole('region', { name: 'Hugging Face' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'More results' })).not.toBeInTheDocument();
  });

  it('keeps showing the search as running until the newest one answers', async () => {
    const answers: ((r: HfSearchResponse) => void)[] = [];
    transport.browseHfModels.mockImplementation(() => new Promise((resolve) => answers.push(resolve)));
    renderLibrary();
    const user = userEvent.setup();
    await user.type(field(), 'first{Enter}');
    await user.clear(field());
    await user.type(field(), 'second{Enter}');
    await waitFor(() => expect(answers).toHaveLength(2));

    await act(async () => answers[0](page(['owner/first'])));
    expect(screen.getByText('Loading models…')).toBeInTheDocument();
    await act(async () => answers[1](page(['owner/second'])));
    expect(screen.queryByText('Loading models…')).not.toBeInTheDocument();
    expect(hfRows()).toEqual(['owner/second']);
  });

  it('draws Chat/Image and Sort only over a search', async () => {
    renderLibrary();
    const user = userEvent.setup();
    await user.click(field());
    const group = await screen.findByRole('region', { name: 'Hugging Face' });
    expect(within(group).queryByRole('group', { name: 'Kind' })).not.toBeInTheDocument();
    expect(within(group).queryByRole('button', { name: 'Sort Hugging Face' })).not.toBeInTheDocument();

    await user.type(field(), 'owner/z-GGUF:Q8_0{Enter}');
    expect(within(await screen.findByRole('region', { name: 'Hugging Face' })).queryByRole('group', { name: 'Kind' })).not.toBeInTheDocument();

    await user.clear(field());
    await user.type(field(), 'llama{Enter}');
    const searched = await screen.findByRole('region', { name: 'Hugging Face' });
    expect(within(searched).getByRole('group', { name: 'Kind' })).toBeInTheDocument();
    expect(within(searched).getByRole('button', { name: 'Sort Hugging Face' })).toBeInTheDocument();
  });

  it('reads Enter: a search, a repo to open, a download, or nothing', () => {
    expect(enterAction('  ')).toBeNull();
    expect(enterAction('qwen 8b')).toEqual({ kind: 'search', query: 'qwen 8b' });
    expect(enterAction('owner/repo')).toEqual({ kind: 'open', repo: 'owner/repo' });
    expect(enterAction('owner/repo:Q8_0')).toEqual({ kind: 'download', repo: 'owner/repo', quant: 'Q8_0' });
    expect(enterAction('https://huggingface.co/owner/repo')).toEqual({ kind: 'open', repo: 'owner/repo' });
  });
});
