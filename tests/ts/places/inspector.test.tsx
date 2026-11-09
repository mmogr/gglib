/**
 * The inspector for a model on this machine, driven by its address and by
 * clicks through the real routes: the fit sentence and a refusal; How it
 * runs as the plan's arguments grouped by decision, each with whose choice
 * it was and why on hover; the launch line, which is the CLI's recorded
 * command line; Load sending nothing typed; Chat loading first; Unload
 * asking the daemon; Change saving one field and the row then reading
 * `set by you · Reset`; the loaded model's state and How it ran; Details
 * remembered; ⋯ Remove and Load with options (the sheet's own tests are
 * `loadOptions.test.tsx`; Verify files and Check for updates are
 * `verification.test.tsx`'s); llama.cpp missing.
 *
 * The transport is a fake; a serve goes through the real API call and
 * mapper to a fake `post`, so the body is the JSON a load sends.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, cleanup, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

const library = vi.hoisted(() => ({ models: [] as unknown[] }));
const explain = vi.hoisted(() => ({ answer: null as unknown, reads: 0 }));
const servers = vi.hoisted(() => ({ list: [] as unknown[] }));
const setup = vi.hoisted(() => ({ status: { llamaInstalled: true, llamaCanDownload: true } as Record<string, unknown> }));
const post = vi.hoisted(() => vi.fn(async (_path: string, _body?: unknown): Promise<unknown> => ({ port: 9456 })));
const install = vi.hoisted(() => ({ onEvent: null as ((e: unknown) => void) | null }));
const transport = vi.hoisted(() => ({
  listModels: vi.fn(async () => library.models),
  listServers: vi.fn(async () => servers.list),
  stopServer: vi.fn(async (_id: number) => {}),
  removeModel: vi.fn(async (_id: number) => {}),
  updateModel: vi.fn(async (_p: Record<string, unknown>) => ({})),
  getDownloadQueue: vi.fn(async () => null),
  listPairedModels: vi.fn(async () => null),
  getModelDetail: vi.fn(async () => null),
  addModelTag: vi.fn(async (_id: number, _tag: string) => {}),
  removeModelTag: vi.fn(async (_id: number, _tag: string) => {}),
}));

vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  const { serveModel } = await import('../../../src/services/transport/api/servers');
  return {
    ...actual,
    getTransport: () => ({
      ...transport,
      serveModel,
      getSettings: vi.fn(async () => ({})),
      subscribe: vi.fn(() => () => {}),
      onEventStreamOpen: vi.fn(() => () => {}),
      revealSecret: vi.fn(async () => null),
    }),
  };
});
vi.mock('../../../src/services/transport/api/client', () => ({
  get: vi.fn(async (path: string) => {
    if (path.includes('/explain')) {
      explain.reads += 1;
      return explain.answer;
    }
    return null;
  }),
  post: (path: string, body?: unknown) => post(path, body),
}));
vi.mock('../../../src/services/transport/api/setup', () => ({
  getSetupStatus: vi.fn(async () => setup.status),
  getEngineStatus: vi.fn(async () => ({ installed: true })),
  streamEngineInstall: vi.fn((_kind: string, onEvent: (e: unknown) => void) => {
    install.onEvent = onEvent;
    return () => {};
  }),
  getRecommendedModel: vi.fn(async () => null),
}));
vi.mock('../../../src/services/remoteEvents', () => ({ refreshRemoteStatus: vi.fn() }));
// A loaded model's ▸ Log reads the server's log; here it has none, and no stream opens.
vi.mock('../../../src/services/platform/serverLogs', () => ({
  getServerLogs: vi.fn(async () => []),
  listenToServerLogs: vi.fn(async () => () => {}),
}));
vi.mock('../../../src/places/chat/ChatPlace', () => ({
  default: ({ modelName, serverPort }: { modelName: string; serverPort?: number }) => (
    <div data-testid="chat-page" data-port={serverPort}>{`Chatting with ${modelName}`}</div>
  ),
}));

import { cleanupModelEvents, initModelEvents } from '../../../src/services/modelEvents';
import { ingestServerEvent } from '../../../src/services/serverRegistry';
import { resetEngineInstall } from '../../../src/places/library/engineInstall';
import { RESIDENCY_WORDS } from '../../../src/components/status/words';
import { appLogger } from '../../../src/services/platform';
import { REASONING_KEYS, SAMPLING_KEYS } from '../../../src/inspector/local/rows';
import type { ContextFit } from '../../../src/types/generated/ContextFit';
import type { LaunchPlan } from '../../../src/types/generated/LaunchPlan';
import { renderApp } from './harness';
import { guiModel } from '../fixtures/model';
import { chatPlan, launchExplanation, samplingExplanation } from '../fixtures/explain';
import { resolvedConfig } from '../fixtures/inference';
import { rust } from '../contracts/rustSource';

const GEMMA = guiModel({ id: 9, name: 'Gemma-3-4B-It', openaiId: 'Gemma-3-4B-It', quantization: 'Q4_K_M', imageInput: true });

function renderInspector(model = GEMMA, path = `/models/${model.id}`) {
  library.models = [model];
  initModelEvents();
  return renderApp(path);
}

const inspector = () => screen.getByRole('complementary', { name: 'Inspector' });

/** The one update sent, its absent keys dropped. */
const sentUpdate = () => {
  expect(transport.updateModel).toHaveBeenCalledTimes(1);
  return Object.fromEntries(Object.entries(transport.updateModel.mock.calls[0][0]).filter(([, v]) => v !== undefined));
};

/** `chatPlan` without its templates flag: a model gglib takes no Jinja position on. */
const noJinja = () => chatPlan({ args: chatPlan().args.filter((a) => a.decision !== 'templates') });
const row = (label: string) => within(inspector()).getByRole('group', { name: label });

/** A clock past every write an earlier test left in the server registry, which orders writes by time. */
let clock = Date.now() + 1e9;

/** A server for model 9, as the registry hears of one. */
function serve(type: 'running' | 'stopped' = 'running') {
  act(() => ingestServerEvent({ type, modelId: '9', port: 9001, updatedAt: (clock += 1e6), modelName: 'Gemma-3-4B-It' }));
}

describe('the inspector, a model on this machine', () => {
  beforeEach(() => {
    explain.answer = launchExplanation(samplingExplanation(), chatPlan());
    explain.reads = 0;
    servers.list = [];
    setup.status = { llamaInstalled: true, llamaCanDownload: true };
    post.mockClear();
    post.mockImplementation(async () => ({ port: 9456 }));
    for (const mock of Object.values(transport)) mock.mockClear();
    resetEngineInstall();
    install.onEvent = null;
    try {
      localStorage.clear();
    } catch {
      // no storage in this environment
    }
  });
  afterEach(() => {
    cleanup();
    cleanupModelEvents();
    serve('stopped');
  });

  it('says the fit sentence the daemon serves, a refusal as a refusal, and the Context row its note', async () => {
    renderInspector();
    expect(await screen.findByText('Fits this machine with 32K context, of 128K trained.')).toBeInTheDocument();
    expect(row('Context')).toHaveTextContent('32,768 · fits (35.81 GiB of 43.20 GiB usable)');
    cleanup();

    // The fit Rust records for a machine whose memory could not be read.
    const recorded = JSON.parse(rust('contracts/models/fit_sentence.json')) as ContextFit[];
    const refused = recorded.find((f) => f.fitted === null && f.budgetBytes === null)!;
    explain.answer = launchExplanation(samplingExplanation(), chatPlan({ fit: refused }));
    renderInspector();
    expect(
      await screen.findByText("Serves at 4K context, of 128K trained; gglib could not fit one to this machine: this machine's memory could not be read."),
    ).toBeInTheDocument();
    // A fit that did not know every figure has no note.
    expect(row('Context').textContent).not.toContain('fit (');
  });

  it('groups the arguments by their decision, each row saying whose choice and why on hover', async () => {
    // The daemon names every field; here none resolves to anything.
    const sources = [...SAMPLING_KEYS, ...REASONING_KEYS].map((param) => ({ param, kind: 'unset' as const }));
    explain.answer = launchExplanation(samplingExplanation({ sources }), chatPlan());
    renderInspector();
    await screen.findByRole('group', { name: 'Context' });

    expect(within(row('Context')).getByText(/^32,768/)).toHaveAttribute(
      'title',
      'gglib: the largest rung that fits beside the weights on this machine',
    );
    expect(within(row('Images')).getByText('mmproj-F16.gguf')).toHaveAttribute('title', 'you: the projector linked to this model');
    expect(within(row('Templates')).getByText('--jinja')).toBeInTheDocument();
    expect(within(row('KV cache')).getByText('--cache-type-k q8_0 · --cache-type-v q8_0')).toBeInTheDocument();
    // The weights and the port are the launch line's, not rows.
    expect(within(inspector()).queryByRole('group', { name: 'model' })).not.toBeInTheDocument();
    expect(within(inspector()).queryByRole('group', { name: 'port' })).not.toBeInTheDocument();
    expect(within(inspector()).getByText('chosen by model author, gglib, you')).toBeInTheDocument();
    // Nothing resolved: every sampler is llama.cpp's.
    expect(within(row('Sampling')).getByText("15 use llama.cpp's defaults")).toBeInTheDocument();
    // One footnote: the rungs the rows cannot show.
    expect(within(inspector()).getByText(/Client sampling is ignored, except max_tokens/)).toBeInTheDocument();
  });

  it('draws the launch line as the CLI records the command line, each token saying why', async () => {
    const recorded = JSON.parse(rust('contracts/models/launch_line.json')) as { plan: LaunchPlan; line: string }[];
    const quoted = recorded[1];
    explain.answer = launchExplanation(samplingExplanation(), { ...quoted.plan, fit: chatPlan().fit });
    renderInspector();

    const line = await screen.findByLabelText('Launch line');
    expect(line.textContent).toBe(quoted.line);

    await userEvent.setup().click(within(line).getByRole('button', { name: "--mmproj '/it'\\''s/mmproj.gguf'" }));
    const note = screen.getByRole('note', { name: '--mmproj' });
    expect(note).toHaveTextContent('why');
    expect(note).toHaveTextContent('chosen by you');
  });

  it('⧉ beside the launch line copies the whole command as one shell line, then says Copied for one second', async () => {
    const recorded = JSON.parse(rust('contracts/models/launch_line.json')) as { plan: LaunchPlan; line: string }[];
    explain.answer = launchExplanation(samplingExplanation(), { ...recorded[1].plan, fit: chatPlan().fit });
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      renderInspector();
      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
      await user.click(await screen.findByRole('button', { name: 'Copy as command' }));

      expect(await navigator.clipboard.readText()).toBe(
        "llama-server -m '/Users/m/Library/Application Support/gglib/a b.gguf' --mmproj '/it'\\''s/mmproj.gguf'" +
          " --alias 'say \"hi\"' --sep 'a\tb' --empty '' --alias '$HOME' -ot 'blk\\.\\d+\\.ffn_.*=CPU' --override-kv k=v,a+b%c:d@e",
      );
      expect(await screen.findByText('Copied')).toBeInTheDocument();
      act(() => vi.advanceTimersByTime(900));
      expect(screen.getByText('Copied')).toBeInTheDocument();
      act(() => vi.advanceTimersByTime(200));
      expect(screen.queryByText('Copied')).not.toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it('loads on one click with nothing typed, and opens no chat', async () => {
    renderInspector();
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Load' }));

    await waitFor(() => expect(post).toHaveBeenCalledTimes(1));
    const [path, body] = post.mock.calls[0];
    expect(path).toBe('/api/servers/start');
    expect(JSON.stringify(body)).toBe('{"id":9,"mlock":false}');
    expect(screen.queryByTestId('chat-page')).not.toBeInTheDocument();
  });

  it('says a failed load in one line, in the daemon’s words', async () => {
    post.mockRejectedValueOnce(new Error('out of memory (needs 9.1 GB, 6.0 free)'));
    renderInspector();
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Load' }));

    expect(await screen.findByRole('alert')).toHaveTextContent("Didn't load: out of memory (needs 9.1 GB, 6.0 free)");
  });

  it('loads a model that is not loaded for Chat, then opens the chat on its port', async () => {
    renderInspector();
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Chat' }));

    const chat = await screen.findByTestId('chat-page');
    expect(chat).toHaveAttribute('data-port', '9456');
    expect(JSON.stringify(post.mock.calls[0][1])).toBe('{"id":9,"mlock":false}');
  });

  it('asks the daemon to unload a loaded model', async () => {
    serve();
    renderInspector();
    const aside = await screen.findByRole('complementary', { name: 'Inspector' });
    await userEvent.setup().click(await within(aside).findByRole('button', { name: 'Unload' }));

    await waitFor(() => expect(transport.stopServer).toHaveBeenCalledWith(9));
  });

  it('says a loaded model’s state and how it was launched, as launched', async () => {
    servers.list = [
      {
        model_id: 9, model_name: 'Gemma-3-4B-It', pid: 1, port: 9001, started_at: 1, runtime: 'llama',
        launch: {
          state: 'working', context_size: 32768, weights_bytes: 1, quantization: 'Q4_K_M', flag_line: '',
          narration: {
            model_name: 'Gemma-3-4B-It', quantization: 'Q4_K_M', weights_bytes: 1,
            decisions: [
              { label: 'ctx', value: '32768', source: 'fitted to hardware' },
              { label: 'kv', value: 'q8_0', source: null },
            ],
          },
        },
      },
    ];
    serve();
    renderInspector();

    expect(await within(await screen.findByRole('complementary', { name: 'Inspector' })).findByText(RESIDENCY_WORDS.working)).toBeInTheDocument();
    const ran = await screen.findByRole('region', { name: 'How it ran' });
    expect(within(ran).getByText('as launched')).toBeInTheDocument();
    expect(within(within(ran).getByRole('group', { name: 'Context' })).getByText('32,768')).toHaveAttribute('title', 'fitted to hardware');
    expect(within(within(ran).getByRole('group', { name: 'KV cache' })).getByText('q8_0')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Load' })).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Launch line')).not.toBeInTheDocument();

    await userEvent.setup().click(within(ran).getByRole('button', { name: 'Change Context' }));
    expect(within(ran).getByText('Applies at next load.')).toBeInTheDocument();
  });

  it('saves only the context it changed, keeping the stored rest, then reads the launch again', async () => {
    const model = guiModel({ ...GEMMA, serverDefaults: { contextLength: null, jinja: true } });
    renderInspector(model);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Change Context' }));
    const reads = explain.reads;
    await user.type(screen.getByRole('spinbutton', { name: 'Context' }), '16384');
    await user.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => expect(transport.updateModel).toHaveBeenCalledTimes(1));
    const sent = Object.fromEntries(Object.entries(transport.updateModel.mock.calls[0][0]).filter(([, v]) => v !== undefined));
    expect(sent).toEqual({ id: 9, serverDefaults: { contextLength: 16384, jinja: true } });
    await waitFor(() => expect(explain.reads).toBeGreaterThan(reads));
  });

  it('reads set by you on a row the person set, and Reset clears that one value', async () => {
    const fit = chatPlan().fit!;
    explain.answer = launchExplanation(
      samplingExplanation({ sources: [{ param: 'topP', kind: 'layer', layer: 'modelUserSet' }] }),
      chatPlan({ fit: { ...fit, source: 'model_server_defaults' } }),
    );
    const model = guiModel({ ...GEMMA, serverDefaults: { contextLength: 16384, jinja: true } });
    renderInspector(model);
    const user = userEvent.setup();
    await screen.findByRole('group', { name: 'Context' });

    expect(row('Context')).toHaveTextContent('set by you');
    expect(row('Templates')).not.toHaveTextContent('set by you');
    expect(row('Sampling')).toHaveTextContent('set by you');
    await user.click(within(row('Context')).getByRole('button', { name: 'Reset' }));

    await waitFor(() => expect(transport.updateModel).toHaveBeenCalledTimes(1));
    expect(transport.updateModel.mock.calls[0][0]).toMatchObject({ id: 9, serverDefaults: { contextLength: null, jinja: true } });
  });

  it('remembers Details open for the next model it shows', async () => {
    const view = renderInspector();
    const user = userEvent.setup();
    await user.click(await screen.findByText('Details'));
    await waitFor(() => expect(screen.getByText('Details').closest('details')).toHaveAttribute('open'));
    view.unmount();

    renderInspector();
    await waitFor(() => expect(screen.getByText('Details').closest('details')).toHaveAttribute('open'));
  });

  it('removes the model from the library after asking, saying the file stays', async () => {
    const { at } = renderInspector();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'More' }));
    await user.click(screen.getByRole('menuitem', { name: 'Remove from library…' }));

    // Cancelled, nothing is removed.
    await user.click(within(await screen.findByRole('dialog', { name: /remove Gemma-3-4B-It/i })).getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(transport.removeModel).not.toHaveBeenCalled();

    await user.click(screen.getByRole('button', { name: 'More' }));
    await user.click(screen.getByRole('menuitem', { name: 'Remove from library…' }));
    const confirm = await screen.findByRole('dialog', { name: /remove Gemma-3-4B-It/i });
    expect(confirm).toHaveTextContent('The file stays on disk.');
    await user.click(within(confirm).getByRole('button', { name: 'Remove' }));
    await waitFor(() => expect(transport.removeModel).toHaveBeenCalledWith(9));
    await waitFor(() => expect(at()).toBe('/library'));
  });

  it('says Copied beside what copied, for one second, and raises no toast', async () => {
    renderInspector();
    const user = userEvent.setup();
    const head = (await screen.findByRole('heading', { name: 'Gemma-3-4B-It' })).parentElement as HTMLElement;
    await user.click(within(head).getByRole('button', { name: 'Copy the Model ID Gemma-3-4B-It' }));
    expect(await navigator.clipboard.readText()).toBe('Gemma-3-4B-It');
    expect(await within(head).findByRole('status')).toHaveTextContent('Copied');
    await waitFor(() => expect(within(head).queryByText('Copied')).not.toBeInTheDocument(), { timeout: 2000 });

    // ⋯ › Copy path closes its menu, so the word is said beside ⋯.
    await user.click(within(head).getByRole('button', { name: 'More' }));
    await user.click(screen.getByRole('menuitem', { name: 'Copy path' }));
    expect(await navigator.clipboard.readText()).toBe('/models/test.gguf');
    expect(await within(head).findByRole('status')).toHaveTextContent('Copied');
    expect(screen.getByTestId('toasts')).toBeEmptyDOMElement();
  });

  it('⋯ › Copy path that failed says no Copied, and is logged', async () => {
    renderInspector();
    const user = userEvent.setup();
    const head = (await screen.findByRole('heading', { name: 'Gemma-3-4B-It' })).parentElement as HTMLElement;
    const complaints = vi.spyOn(appLogger, 'error').mockImplementation(() => {});
    const refused = new Error('clipboard refused');
    vi.spyOn(navigator.clipboard, 'writeText').mockRejectedValueOnce(refused);
    try {
      await user.click(within(head).getByRole('button', { name: 'More' }));
      await user.click(screen.getByRole('menuitem', { name: 'Copy path' }));
      await waitFor(() => expect(complaints).toHaveBeenCalledWith('component.model', 'Failed to copy the path', { error: refused }));
      expect(within(head).queryByText('Copied')).not.toBeInTheDocument();
    } finally {
      complaints.mockRestore();
    }
  });

  it('opens the Load with options sheet from Load with options…', async () => {
    renderInspector();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'More' }));
    await user.click(screen.getByRole('menuitem', { name: 'Load with options…' }));
    expect(await screen.findByRole('dialog', { name: 'Load with options' })).toBeInTheDocument();
  });

  it('installs llama.cpp in place of Load when it is missing, then loads', async () => {
    setup.status = { llamaInstalled: false, llamaCanDownload: true };
    renderInspector();
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Install llama.cpp' }));
    expect(install.onEvent).not.toBeNull();
    expect(post).not.toHaveBeenCalled();

    setup.status = { llamaInstalled: true, llamaCanDownload: true };
    act(() => install.onEvent?.({ type: 'completed' }));
    await waitFor(() => expect(post).toHaveBeenCalledTimes(1));
    expect(JSON.stringify(post.mock.calls[0][1])).toBe('{"id":9,"mlock":false}');
  });

  it("says each row in the narration's words where it has them, keeping the arguments' why", async () => {
    const plan = chatPlan({
      narration: {
        model_name: 'Gemma-3-4B-It', quantization: 'Q4_K_M', weights_bytes: 1,
        decisions: [
          { label: 'ctx', value: '32768', source: 'fitted to hardware' },
          { label: 'kv', value: 'q8_0 -> 9.6 GiB, f16 would be 18.0 GiB', source: 'default' },
          { label: 'flags', value: '--jinja (agent tag)', source: null },
        ],
      },
    });
    explain.answer = launchExplanation(samplingExplanation(), plan);
    renderInspector();
    await screen.findByRole('group', { name: 'Context' });

    expect(within(row('KV cache')).getByText('q8_0 -> 9.6 GiB, f16 would be 18.0 GiB')).toHaveAttribute(
      'title',
      'gglib: q8_0 halves the KV cache at no measured cost',
    );
    expect(row('Templates')).not.toHaveTextContent('(agent tag)');
    expect(within(row('Context')).getByText(/^32,768 · fits/)).toBeInTheDocument();
    // A row the narration says nothing of reads its arguments.
    expect(within(row('Images')).getByText('mmproj-F16.gguf')).toBeInTheDocument();
  });

  it('draws Templates as Auto when gglib sends no flag, and Change saves On with the stored rest', async () => {
    // The narration's flags line names the reasoning flag; it is not Templates' value.
    const narration = { model_name: 'Gemma-3-4B-It', quantization: null, weights_bytes: 1, decisions: [{ label: 'flags', value: '--reasoning-format deepseek (reasoning tag)', source: null }] };
    explain.answer = launchExplanation(samplingExplanation(), { ...noJinja(), narration });
    renderInspector(guiModel({ ...GEMMA, serverDefaults: { contextLength: 16384 } }));
    const user = userEvent.setup();
    await screen.findByRole('group', { name: 'Templates' });

    expect(within(row('Templates')).getByText('Auto')).toBeInTheDocument();
    expect(row('Templates')).not.toHaveTextContent('set by you');
    await user.click(within(row('Templates')).getByRole('button', { name: 'Change Templates' }));
    await user.selectOptions(screen.getByRole('combobox', { name: 'Templates' }), 'On');

    await waitFor(() => expect(sentUpdate()).toEqual({ id: 9, serverDefaults: { contextLength: 16384, jinja: true } }));
  });

  it('keeps Speculative turned off in view, set by you, and Reset and Change keep the stored rest', async () => {
    explain.answer = launchExplanation(samplingExplanation(), noJinja());
    renderInspector(guiModel({ ...GEMMA, serverDefaults: { contextLength: 16384, mtp: false } }));
    const user = userEvent.setup();
    await screen.findByRole('group', { name: 'Speculative' });

    expect(within(row('Speculative')).getByText('Off')).toBeInTheDocument();
    await user.click(within(row('Speculative')).getByRole('button', { name: 'Reset' }));
    await waitFor(() => expect(sentUpdate()).toEqual({ id: 9, serverDefaults: { contextLength: 16384, mtp: null } }));

    transport.updateModel.mockClear();
    await user.click(within(row('Speculative')).getByRole('button', { name: 'Change Speculative' }));
    await user.selectOptions(screen.getByRole('combobox', { name: 'Speculative' }), 'On');
    await waitFor(() => expect(sentUpdate()).toEqual({ id: 9, serverDefaults: { contextLength: 16384, mtp: true } }));
  });

  it('saves a sampling value added in place, with the stored values kept', async () => {
    const stored = resolvedConfig({ temperature: 0.7, reasoningEffort: 'high' });
    renderInspector(guiModel({ ...GEMMA, inferenceDefaults: stored }));
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Change Sampling' }));
    // Only the set field is drawn until one is added.
    expect(screen.getByLabelText('Temperature')).toBeInTheDocument();
    expect(screen.queryByLabelText('Top K')).not.toBeInTheDocument();
    await user.selectOptions(screen.getByRole('combobox', { name: 'Add a value' }), 'Top K');
    await user.type(screen.getByLabelText('Top K'), '40');
    await user.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => expect(sentUpdate()).toEqual({ id: 9, inferenceDefaults: { ...stored, topK: 40 } }));
  });

  it('saves a reasoning value added in place, with the stored values kept', async () => {
    const stored = resolvedConfig({ temperature: 0.7 });
    renderInspector(guiModel({ ...GEMMA, inferenceDefaults: stored }));
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Change Reasoning' }));
    await user.selectOptions(screen.getByRole('combobox', { name: 'Add a value' }), 'Reasoning Budget');
    await user.type(screen.getByLabelText('Reasoning Budget'), '512');
    await user.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => expect(sentUpdate()).toEqual({ id: 9, inferenceDefaults: { ...stored, reasoningBudgetTokens: 512 } }));
  });

  it("resets the sampling values you set and leaves the reasoning ones", async () => {
    explain.answer = launchExplanation(samplingExplanation({ sources: [{ param: 'topP', kind: 'layer', layer: 'modelUserSet' }] }), chatPlan());
    renderInspector(guiModel({ ...GEMMA, inferenceDefaults: resolvedConfig({ topP: 0.9, reasoningEffort: 'high' }) }));
    await screen.findByRole('group', { name: 'Sampling' });
    await userEvent.setup().click(within(row('Sampling')).getByRole('button', { name: 'Reset' }));

    await waitFor(() => expect(transport.updateModel).toHaveBeenCalledTimes(1));
    const sent = transport.updateModel.mock.calls[0][0].inferenceDefaults as Record<string, unknown>;
    expect(sent).not.toHaveProperty('topP');
    expect(sent.reasoningEffort).toBe('high');
  });

  it('says a failed unload as an unload', async () => {
    transport.stopServer.mockRejectedValueOnce(new Error('the daemon went away'));
    serve();
    renderInspector();
    const aside = await screen.findByRole('complementary', { name: 'Inspector' });
    await userEvent.setup().click(await within(aside).findByRole('button', { name: 'Unload' }));

    expect(await screen.findByRole('alert')).toHaveTextContent("Didn't unload: the daemon went away");
  });

  it('reads the launch again after a tag is added or the path changed in Details', async () => {
    localStorage.setItem('gglib.inspector.details', 'open');
    renderInspector();
    const user = userEvent.setup();
    await screen.findByRole('group', { name: 'Context' });
    let reads = explain.reads;
    await user.type(screen.getByRole('textbox', { name: 'Add a tag' }), 'agent{Enter}');
    await waitFor(() => expect(transport.addModelTag).toHaveBeenCalledWith(9, 'agent'));
    await waitFor(() => expect(explain.reads).toBeGreaterThan(reads));

    reads = explain.reads;
    await user.click(screen.getByRole('button', { name: 'Change path' }));
    const field = screen.getByRole('textbox', { name: 'path' });
    await user.clear(field);
    await user.type(field, '/models/other.gguf{Enter}');
    await waitFor(() => expect(sentUpdate()).toEqual({ id: 9, filePath: '/models/other.gguf' }));
    await waitFor(() => expect(explain.reads).toBeGreaterThan(reads));
  });

  it('reads the launch again after a tag is removed in Details', async () => {
    localStorage.setItem('gglib.inspector.details', 'open');
    transport.getModelDetail.mockResolvedValue({ tags: ['agent'] } as never);
    renderInspector();
    const user = userEvent.setup();
    const remove = await screen.findByRole('button', { name: 'Remove tag agent' });
    const reads = explain.reads;
    await user.click(remove);

    await waitFor(() => expect(transport.removeModelTag).toHaveBeenCalledWith(9, 'agent'));
    await waitFor(() => expect(explain.reads).toBeGreaterThan(reads));
    transport.getModelDetail.mockResolvedValue(null);
  });

  it('says a refused llama.cpp install as a failed load', async () => {
    setup.status = { llamaInstalled: false, llamaCanDownload: true };
    renderInspector();
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Install llama.cpp' }));
    act(() => install.onEvent?.({ type: 'failed', message: 'no build for this Mac' }));

    expect(await screen.findByRole('alert')).toHaveTextContent("Didn't load: no build for this Mac");
    expect(post).not.toHaveBeenCalled();
  });

  it('reads the launch again after the tags are re-detected', async () => {
    renderInspector();
    const user = userEvent.setup();
    await screen.findByRole('group', { name: 'Context' });
    const reads = explain.reads;
    await user.click(screen.getByRole('button', { name: 'More' }));
    await user.click(screen.getByRole('menuitem', { name: 'Re-detect tags' }));

    await waitFor(() => expect(post.mock.calls.map(([path]) => path)).toContain('/api/models/9/retag'));
    await waitFor(() => expect(explain.reads).toBeGreaterThan(reads));
  });

  it('draws nothing of How it runs before the answer, and says it is unavailable when there is none', async () => {
    explain.answer = null;
    renderInspector();
    expect(await screen.findByText('How this model would load is unavailable.')).toBeInTheDocument();
    cleanup();

    explain.answer = new Promise(() => {});
    renderInspector();
    await screen.findByRole('button', { name: 'Load' });
    expect(screen.queryByRole('region', { name: 'How it runs' })).not.toBeInTheDocument();
  });

  it('keeps Load off for an image model with a part missing, and says which', async () => {
    renderInspector(guiModel({ id: 9, name: 'flux-schnell', imageFamily: 'flux1', missingComponents: ['clip_l'] }));

    expect(await screen.findByRole('button', { name: 'Load' })).toBeDisabled();
    expect(within(inspector()).getByText('▲ CLIP-L missing.')).toBeInTheDocument();
  });

  it('keeps the name and the main actions in view, and draws Remove as a danger', async () => {
    renderInspector();
    const user = userEvent.setup();
    expect((await screen.findByRole('button', { name: 'Load' })).closest('.sticky')).toContainElement(
      screen.getByRole('heading', { name: 'Gemma-3-4B-It' }),
    );
    await user.click(screen.getByRole('button', { name: 'More' }));
    expect(screen.getByRole('menuitem', { name: 'Remove from library…' })).toHaveClass('text-danger');
    expect(screen.getByRole('menuitem', { name: 'Verify files' })).not.toHaveClass('text-danger');
  });
});
