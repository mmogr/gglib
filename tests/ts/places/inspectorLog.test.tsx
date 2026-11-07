/**
 * A loaded model's log in the inspector, and the inspector's width, through
 * the real routes: `/models/:id/log` with `‹ <name> · Log`, each line after
 * its time since the first; Following and Paused as a word switch; ⧉
 * copying every line; ⋯ Clear view; ‹ back to the model. The loaded
 * model's inspector shows `▸ Log` with the server's last line, and a failed
 * load's line offers Show log. The left edge drags the width between 320
 * and 720 px, remembered; the log opens at 60% of the window and leaving
 * it returns to the remembered width.
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
vi.mock('../../../src/pages/ChatPage', () => ({ default: () => <div data-testid="chat-page" /> }));

import { cleanupModelEvents, initModelEvents } from '../../../src/services/modelEvents';
import { ingestServerEvent } from '../../../src/services/serverRegistry';
import { getServerLogs } from '../../../src/services/platform/serverLogs';
import { renderApp } from './harness';
import { guiModel } from '../fixtures/model';

const GEMMA = guiModel({ id: 9, name: 'Gemma-3-4B-It', openaiId: 'Gemma-3-4B-It' });
const line = (timestamp: number, text: string) => ({ timestamp, line: text, port: 9001 });

let clock = Date.now() + 2e9;
function serve(type: 'running' | 'stopped' = 'running') {
  act(() => ingestServerEvent({ type, modelId: '9', port: 9001, updatedAt: (clock += 1e6), modelName: 'Gemma-3-4B-It' }));
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

    await user.click(within(inspector()).getByRole('button', { name: 'More' }));
    await user.click(screen.getByRole('menuitem', { name: 'Clear view' }));
    expect(log).toHaveTextContent('Waiting for server output');
    act(() => logs.push?.(line(3_000, 'next line')));
    expect(log).toHaveTextContent('next line');
  });

  it('says so for a model that has not run since gglib started', async () => {
    library.models = [guiModel({ id: 12, name: 'Phi-4' })];
    initModelEvents();
    renderApp('/models/12/log');
    expect(await within(inspector()).findByText('Phi-4 has not run since gglib started; its log starts when it loads.')).toBeInTheDocument();
    expect(screen.queryByRole('log')).not.toBeInTheDocument();
  });

  it("shows ▸ Log on a loaded model's inspector with the server's last line, and opens the log", async () => {
    serve();
    const { at } = renderAt('/models/9');
    const open = await within(inspector()).findByRole('button', { name: /^Log/ });
    await waitFor(() => expect(open).toHaveTextContent('warn: slow srv llama_server: model loaded'));
    await userEvent.setup().click(open);
    expect(at()).toBe('/models/9/log');
  });

  it("offers Show log on a failed load's line, which opens the log", async () => {
    transport.serveModel.mockRejectedValueOnce(new Error('out of memory'));
    const { at } = renderAt('/models/9');
    const user = userEvent.setup();
    await user.click(await within(inspector()).findByRole('button', { name: 'Load' }));
    const failed = await screen.findByRole('alert');
    expect(failed).toHaveTextContent("Didn't load: out of memory");
    await user.click(within(failed).getByRole('button', { name: 'Show log' }));
    expect(at()).toBe('/models/9/log');
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

    view.unmount();
    cleanupModelEvents();
    renderAt('/models/9');
    await within(inspector()).findByRole('heading', { name: 'Gemma-3-4B-It' });
    expect(inspector()).toHaveStyle({ width: '704px' });
  });

  it('opens the log at 60% of the window, and returns to the remembered width on leaving it', async () => {
    localStorage.setItem('gglib.inspector.width', '420');
    const { router } = renderAt('/models/9');
    await within(inspector()).findByRole('heading', { name: 'Gemma-3-4B-It' });
    expect(inspector()).toHaveStyle({ width: '420px' });

    await act(() => router.navigate('/models/9/log'));
    expect(inspector()).toHaveStyle({ width: `${Math.round(Math.min(720, Math.max(320, window.innerWidth * 0.6)))}px` });
    fireEvent.keyDown(edge(), { key: 'ArrowRight' });

    await act(() => router.navigate('/models/9'));
    expect(inspector()).toHaveStyle({ width: '420px' });
    expect(localStorage.getItem('gglib.inspector.width')).toBe('420');
  });
});
