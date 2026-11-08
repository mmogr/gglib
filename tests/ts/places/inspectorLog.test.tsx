/**
 * A loaded model's log in the inspector, and the inspector's width, through
 * the real routes: `/models/:id/log` with `‹ <name> · Log`, each line after
 * its time since the first unless llama.cpp stamped it; the lines kept once
 * the server stops or crashes (events that name no port, as the daemon's
 * do); Following and Paused as a word switch that also says a pause made by
 * scrolling away; ⧉ copying every line; ⋯ Clear view; ‹ back to the model.
 * The loaded model's inspector shows `▸ Log` with the server's last line
 * (cut with an ellipsis) and its llama-server port in Details; a failed
 * load's Show log shows the last known port's lines, or says that start's
 * output is not available. The left edge drags the width between 320 and
 * 720 px, remembered, a stored width outside them held to them; the log
 * opens at 60% of the window, bounded by the window (400 px left for the
 * rest), not by 720, and leaving it returns to the remembered width.
 *
 * The server log's snapshot and stream are fakes the test feeds.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

const library = vi.hoisted(() => ({ models: [] as unknown[] }));
const logs = vi.hoisted(() => ({ snapshot: [] as unknown[], push: null as ((e: unknown) => void) | null }));
const transport = vi.hoisted(() => ({
  listModels: vi.fn(async () => library.models),
  listServers: vi.fn(async () => []),
  stopServer: vi.fn(async (_id: number) => {}),
  getDownloadQueue: vi.fn(async () => null),
  listPairedModels: vi.fn(async () => null),
  getModelDetail: vi.fn(async () => null),
  serveModel: vi.fn(async (_c: unknown): Promise<{ port: number }> => ({ port: 9456 })),
}));

vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return {
    ...actual,
    getTransport: () => ({
      ...transport,
      getSettings: vi.fn(async () => ({})),
      subscribe: vi.fn(() => () => {}),
      onEventStreamOpen: vi.fn(() => () => {}),
      revealSecret: vi.fn(async () => null),
    }),
  };
});
vi.mock('../../../src/services/transport/api/client', () => ({
  get: vi.fn(async (path: string) => {
    if (!path.includes('/explain')) return null;
    const { chatPlan, launchExplanation, samplingExplanation } = await import('../fixtures/explain');
    return launchExplanation(samplingExplanation(), chatPlan());
  }),
}));
vi.mock('../../../src/services/transport/api/setup', () => ({
  getSetupStatus: vi.fn(async () => ({ llamaInstalled: true, llamaCanDownload: true })),
  getRecommendedModel: vi.fn(async () => null),
}));
vi.mock('../../../src/services/platform/serverLogs', () => ({
  getServerLogs: vi.fn(async () => logs.snapshot),
  listenToServerLogs: vi.fn(async (_port: number, push: (e: unknown) => void) => {
    logs.push = push;
    return () => {
      logs.push = null;
    };
  }),
}));
vi.mock('../../../src/services/remoteEvents', () => ({ refreshRemoteStatus: vi.fn() }));
vi.mock('../../../src/places/chat/ChatPlace', () => ({ default: () => <div data-testid="chat-page" /> }));

import { cleanupModelEvents, initModelEvents } from '../../../src/services/modelEvents';
import { ingestServerEvent } from '../../../src/services/serverRegistry';
import { getServerLogs } from '../../../src/services/platform/serverLogs';
import { appLogger } from '../../../src/services/platform';
import { renderApp } from './harness';
import { guiModel } from '../fixtures/model';

const GEMMA = guiModel({ id: 9, name: 'Gemma-3-4B-It', openaiId: 'Gemma-3-4B-It' });
const line = (timestamp: number, text: string) => ({ timestamp, line: text, port: 9001 });

let clock = Date.now() + 2e9;
/** Model 9's server as the daemon announces it: a start names its port, a stop or a crash none. */
function serve(type: 'running' | 'stopped' | 'crashed' = 'running') {
  const port = type === 'running' ? { port: 9001 } : {};
  act(() => ingestServerEvent({ type, modelId: '9', ...port, updatedAt: (clock += 1e6), modelName: 'Gemma-3-4B-It' }));
}

/** Set the window's width for a test, giving back a function that restores it. */
function windowWidth(px: number): () => void {
  const was = window.innerWidth;
  Object.defineProperty(window, 'innerWidth', { configurable: true, value: px });
  return () => Object.defineProperty(window, 'innerWidth', { configurable: true, value: was });
}

function renderAt(path: string) {
  library.models = [GEMMA];
  initModelEvents();
  return renderApp(path);
}

const inspector = () => screen.getByRole('complementary', { name: 'Inspector' });
const edge = () => screen.getByRole('separator', { name: 'Inspector width' });

describe('the log, in the inspector', () => {
  beforeEach(() => {
    logs.snapshot = [line(1_000, 'main: build 4321'), line(2_250, '\u001b[33mwarn: slow\u001b[0m srv  llama_server: model loaded')];
    for (const mock of Object.values(transport)) mock.mockClear();
    try {
      localStorage.clear();
    } catch {
      // no storage here
    }
  });
  afterEach(() => {
    cleanupModelEvents();
    serve('stopped');
  });

  it('is at /models/:id/log: its name, each line after its time since the first, and ‹ back to the model', async () => {
    serve();
    const { at } = renderAt('/models/9/log');
    expect(await within(inspector()).findByRole('heading', { name: 'Gemma-3-4B-It · Log' })).toBeInTheDocument();
    const log = await screen.findByRole('log', { name: 'Server output' });
    await waitFor(() => expect(log).toHaveTextContent('0:00.000main: build 4321'));
    expect(log).toHaveTextContent('0:01.250warn: slow srv llama_server: model loaded');
    expect(getServerLogs).toHaveBeenCalledWith(9001);

    await userEvent.setup().click(within(inspector()).getByRole('button', { name: 'Back to Gemma-3-4B-It' }));
    expect(at()).toBe('/models/9');
  });

  it('a copy of the log that failed says no Copied, and is logged', async () => {
    serve();
    renderAt('/models/9/log');
    const user = userEvent.setup();
    const log = await screen.findByRole('log', { name: 'Server output' });
    await waitFor(() => expect(log).toHaveTextContent('main: build 4321'));
    const complaints = vi.spyOn(appLogger, 'error').mockImplementation(() => {});
    vi.spyOn(navigator.clipboard, 'writeText').mockRejectedValueOnce(new Error('clipboard refused'));
    try {
      await user.click(within(inspector()).getByRole('button', { name: 'Copy the log' }));
      await waitFor(() => expect(complaints).toHaveBeenCalledWith('hook.server', 'Failed to copy logs', expect.anything()));
      expect(within(inspector()).queryByText('Copied')).not.toBeInTheDocument();
    } finally {
      complaints.mockRestore();
    }
  });

  it('switches between Following and Paused, copies every line, and clears the view', async () => {
    serve();
    renderAt('/models/9/log');
    const user = userEvent.setup();
    const log = await screen.findByRole('log', { name: 'Server output' });
    await waitFor(() => expect(log).toHaveTextContent('main: build 4321'));
    const following = within(inspector()).getByRole('button', { name: 'Following' });
    const paused = within(inspector()).getByRole('button', { name: 'Paused' });
    expect(following).toHaveAttribute('aria-pressed', 'true');

    await user.click(paused);
    expect(paused).toHaveAttribute('aria-pressed', 'true');
    expect(following).toHaveAttribute('aria-pressed', 'false');

    await user.click(within(inspector()).getByRole('button', { name: 'Copy the log' }));
    await waitFor(async () => expect(await navigator.clipboard.readText()).toBe(logs.snapshot.map((e) => (e as { line: string }).line).join('\n')));
    expect(await within(inspector()).findByText('Copied')).toBeInTheDocument();

    await user.click(within(inspector()).getByRole('button', { name: 'More' }));
    await user.click(screen.getByRole('menuitem', { name: 'Clear view' }));
    expect(log).toHaveTextContent('Waiting for server output');
    act(() => logs.push?.(line(3_000, 'next line')));
    expect(log).toHaveTextContent('next line');
  });

  it.each(['stopped', 'crashed'] as const)('keeps its lines once the server has %s, though the event names no port', async (end) => {
    serve();
    renderAt('/models/9/log');
    const log = await screen.findByRole('log', { name: 'Server output' });
    await waitFor(() => expect(log).toHaveTextContent('main: build 4321'));

    serve(end);
    expect(screen.getByRole('log', { name: 'Server output' })).toHaveTextContent('main: build 4321');
    expect(screen.queryByText(/has not run/)).not.toBeInTheDocument();
  });

  it("drops the GUI's time on a line llama.cpp stamped itself: one time per line", async () => {
    logs.snapshot = [line(1_000, '0.00.046.817 I srv  load_model: loading'), line(1_500, 'plain line')];
    serve();
    renderAt('/models/9/log');
    const log = await screen.findByRole('log', { name: 'Server output' });
    await waitFor(() => expect(log).toHaveTextContent('plain line'));
    const [stamped, plain] = within(log).getAllByText(/load_model|plain line/).map((el) => el.closest('.console-log-line')!);
    expect(stamped.textContent).toBe('0.00.046.817 I srv  load_model: loading');
    expect(plain.textContent).toBe('0:00.500plain line');
  });

  it('says Paused when scrolling away from the end pauses it, and Following when scrolling back resumes', async () => {
    serve();
    renderAt('/models/9/log');
    const log = await screen.findByRole('log', { name: 'Server output' });
    Object.defineProperty(log, 'scrollHeight', { configurable: true, value: 1000 });
    Object.defineProperty(log, 'clientHeight', { configurable: true, value: 100 });
    log.scrollTop = 0;
    fireEvent.scroll(log);
    expect(within(inspector()).getByRole('button', { name: 'Paused' })).toHaveAttribute('aria-pressed', 'true');

    log.scrollTop = 900;
    fireEvent.scroll(log);
    expect(within(inspector()).getByRole('button', { name: 'Following' })).toHaveAttribute('aria-pressed', 'true');
  });

  it("never shows another model's log under this one when the daemon gave its port to that model", async () => {
    serve();
    renderAt('/models/9/log');
    await screen.findByRole('log', { name: 'Server output' });
    serve('stopped');
    act(() => ingestServerEvent({ type: 'running', modelId: '77', port: 9001, updatedAt: (clock += 1e6), modelName: 'Other' }));

    expect(await within(inspector()).findByText("Gemma-3-4B-It's last log is gone: its port now serves another model.")).toBeInTheDocument();
    expect(screen.queryByRole('log')).not.toBeInTheDocument();
    // The registry outlives the test: the other model leaves, as a stop names no port.
    act(() => ingestServerEvent({ type: 'stopped', modelId: '77', updatedAt: (clock += 1e6), modelName: 'Other' }));
  });

  it('keeps a pause made with the switch when scrolling reaches the end', async () => {
    serve();
    renderAt('/models/9/log');
    const log = await screen.findByRole('log', { name: 'Server output' });
    await userEvent.setup().click(within(inspector()).getByRole('button', { name: 'Paused' }));
    Object.defineProperty(log, 'scrollHeight', { configurable: true, value: 1000 });
    Object.defineProperty(log, 'clientHeight', { configurable: true, value: 100 });
    log.scrollTop = 900;
    fireEvent.scroll(log);
    expect(within(inspector()).getByRole('button', { name: 'Paused' })).toHaveAttribute('aria-pressed', 'true');
  });

  it('says so for a model that is not loaded and has no earlier log in this window', async () => {
    library.models = [guiModel({ id: 12, name: 'Phi-4' })];
    initModelEvents();
    renderApp('/models/12/log');
    expect(await within(inspector()).findByText('Phi-4 is not loaded, and this window has no earlier log of it. Its log shows from its next load.')).toBeInTheDocument();
    expect(screen.queryByRole('log')).not.toBeInTheDocument();
  });

  it("shows ▸ Log on a loaded model's inspector with the server's last line, and its port in Details", async () => {
    localStorage.setItem('gglib.inspector.details', 'open');
    serve();
    const { at } = renderAt('/models/9');
    const row = await within(inspector()).findByRole('group', { name: 'Log' });
    const last = await within(row).findByText('warn: slow srv llama_server: model loaded');
    expect(last).toHaveAttribute('title', 'warn: slow srv  llama_server: model loaded');
    expect(last).toHaveClass('truncate');
    expect(within(inspector()).getByText('port 9001, internal')).toBeInTheDocument();
    await userEvent.setup().click(within(row).getByRole('button', { name: 'Log' }));
    expect(at()).toBe('/models/9/log');
  });

  it("offers Show log on a failed load's line, which shows the lines of the port last known", async () => {
    serve();
    serve('crashed');
    transport.serveModel.mockRejectedValueOnce(new Error('out of memory'));
    const { at } = renderAt('/models/9');
    const user = userEvent.setup();
    await user.click(await within(inspector()).findByRole('button', { name: 'Load' }));
    const failed = await screen.findByRole('alert');
    expect(failed).toHaveTextContent("Didn't load: out of memory");
    await user.click(within(failed).getByRole('button', { name: 'Show log' }));
    expect(at()).toBe('/models/9/log');
    expect(await screen.findByRole('log', { name: 'Server output' })).toHaveTextContent('main: build 4321');
  });

  it('says a failed first start\'s output is not available, where the daemon named no port for it', async () => {
    library.models = [guiModel({ id: 13, name: 'Mistral' })];
    initModelEvents();
    transport.serveModel.mockRejectedValueOnce(new Error('out of memory'));
    renderApp('/models/13');
    const user = userEvent.setup();
    await user.click(await within(inspector()).findByRole('button', { name: 'Load' }));
    await user.click(within(await screen.findByRole('alert')).getByRole('button', { name: 'Show log' }));
    expect(await within(inspector()).findByText('The output of this start is not available: the daemon named no port for it.')).toBeInTheDocument();
    expect(screen.queryByText(/has not run/)).not.toBeInTheDocument();
  });
});

describe("the inspector's width", () => {
  beforeEach(() => {
    logs.snapshot = [];
    try {
      localStorage.clear();
    } catch {
      // no storage here
    }
  });
  afterEach(() => cleanupModelEvents());

  it('drags by its left edge, held to 320–720 px, and is remembered', async () => {
    const view = renderAt('/models/9');
    await within(inspector()).findByRole('heading', { name: 'Gemma-3-4B-It' });
    expect(inspector()).toHaveStyle({ width: '380px' });

    fireEvent.pointerDown(edge(), { clientX: window.innerWidth - 380, pointerId: 1 });
    fireEvent.pointerMove(edge(), { clientX: window.innerWidth - 500, pointerId: 1 });
    fireEvent.pointerUp(edge(), { pointerId: 1 });
    expect(inspector()).toHaveStyle({ width: '500px' });
    fireEvent.pointerMove(edge(), { clientX: 0, pointerId: 1 });
    expect(inspector()).toHaveStyle({ width: '500px' });

    fireEvent.pointerDown(edge(), { clientX: window.innerWidth - 500, pointerId: 1 });
    fireEvent.pointerMove(edge(), { clientX: window.innerWidth - 10, pointerId: 1 });
    expect(inspector()).toHaveStyle({ width: '320px' });
    fireEvent.pointerMove(edge(), { clientX: window.innerWidth - 2000, pointerId: 1 });
    fireEvent.pointerUp(edge(), { pointerId: 1 });
    expect(inspector()).toHaveStyle({ width: '720px' });
    fireEvent.keyDown(edge(), { key: 'ArrowRight' });
    expect(edge()).toHaveAttribute('aria-valuenow', '704');
    fireEvent.keyDown(edge(), { key: 'ArrowRight' });
    fireEvent.keyDown(edge(), { key: 'ArrowLeft' });
    expect(edge()).toHaveAttribute('aria-valuenow', '704');

    view.unmount();
    cleanupModelEvents();
    renderAt('/models/9');
    await within(inspector()).findByRole('heading', { name: 'Gemma-3-4B-It' });
    expect(inspector()).toHaveStyle({ width: '704px' });
  });

  it.each([
    ['5000', '720px'],
    ['100', '320px'],
  ])('holds a stored width of %s to %s', async (stored, shown) => {
    localStorage.setItem('gglib.inspector.width', stored);
    renderAt('/models/9');
    await within(inspector()).findByRole('heading', { name: 'Gemma-3-4B-It' });
    expect(inspector()).toHaveStyle({ width: shown });
  });

  it('opens the log at 60% of a wide window, past 720 px, and returns to the remembered width on leaving it', async () => {
    const restore = windowWidth(1600);
    try {
      localStorage.setItem('gglib.inspector.width', '420');
      const { router } = renderAt('/models/9');
      await within(inspector()).findByRole('heading', { name: 'Gemma-3-4B-It' });
      expect(inspector()).toHaveStyle({ width: '420px' });

      await act(() => router.navigate('/models/9/log'));
      expect(inspector()).toHaveStyle({ width: '960px' });
      // A drag in the log keeps to the window, leaving 400 px for the rest.
      fireEvent.pointerDown(edge(), { clientX: 640, pointerId: 1 });
      fireEvent.pointerMove(edge(), { clientX: 100, pointerId: 1 });
      fireEvent.pointerUp(edge(), { pointerId: 1 });
      expect(inspector()).toHaveStyle({ width: '1200px' });

      await act(() => router.navigate('/models/9'));
      expect(inspector()).toHaveStyle({ width: '420px' });
      expect(localStorage.getItem('gglib.inspector.width')).toBe('420');
    } finally {
      restore();
    }
  });

  it('leaves 400 px of a narrow window when the log opens', async () => {
    const restore = windowWidth(900);
    try {
      renderAt('/models/9/log');
      await within(inspector()).findByRole('heading', { name: 'Gemma-3-4B-It · Log' });
      expect(inspector()).toHaveStyle({ width: '500px' });
    } finally {
      restore();
    }
  });
});
