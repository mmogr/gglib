/**
 * Leaving a chat and unloading its model are two different wishes (#1211),
 * and neither locks the chat.
 *
 * The proxy serves the chat's model to every client, Copilot included, so
 * leaving the chat (the sidebar navigates; there is no Close) stops nothing,
 * and Unload <model>, in the model picker ▾, is the one control here that
 * stops the model (the model's inspector has its own). The place hands it
 * up to the chat route, which `tests/ts/places/libraryChat.test.tsx` pins
 * (CH-10).
 *
 * The composer is never locked. A send loads the chat's model when it is
 * not loaded and turns the endpoint on when it is off; while either is so,
 * one line above the live composer says what a send will do, in Rust's
 * words (`contracts/vocabulary/send_line.json`), with no banner, no toast
 * and no Load button. A crash is the same line, marked, with Show log.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import type { ReactNode } from 'react';
import { agentRun, chatTransport, conversation, framesThenWait, modelPicker, wrapper as pageWrapper } from './chatPlaceHarness';

const transport = vi.hoisted(() => ({ current: {} as unknown }));
vi.mock('../../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../../src/services/transport');
  return { ...actual, getTransport: () => transport.current };
});

import ChatPlace from '../../../../src/places/chat/ChatPlace';
import { ingestServerEvent } from '../../../../src/services/serverRegistry';
import { cleanupServerEvents, forgetServerList, initServerEvents } from '../../../../src/services/serverEvents';
import { applyProxyStatus, resetProxyState } from '../../../../src/services/proxyRegistry';
import { useToastContext } from '../../../../src/contexts/ToastContext';
import type { LocalTurn } from '../../../../src/types/generated/LocalTurn';

/** The toasts, which `ToastProvider` holds but does not draw. */
const ToastProbe = () => {
  const { toasts } = useToastContext();
  return <div data-testid="toasts">{toasts.map((t) => t.message).join(' | ')}</div>;
};
const wrapper = ({ children }: { children: ReactNode }) =>
  pageWrapper({ children: <><ToastProbe />{children}</> });

const onUnloadModel = vi.fn(async () => {});
const onSwitchModel = vi.fn(async (_choice: { modelId: number; modelName: string }) => {});
const onShowModel = vi.fn();
const onShowLog = vi.fn();

const ON = { running: true, state: 'on' as const, port: 9946, base_url: 'http://127.0.0.1:9946/v1', key: 'none_needed' as const, pinned_model: null };
const OFF = { running: false, state: 'off' as const, port: null, base_url: null, key: null, pinned_model: null };

/** The frames a started run is read as; none until a test says. */
let runFrames: object[];
/** The bodies of the runs started, in order. */
let started: LocalTurn[];
let clock = Date.now();

function renderLocal(modelId: number) {
  return render(
    <ChatPlace
      modelName="qwen3"
      modelId={modelId}
      serverPort={4321}
      onSwitchModel={onSwitchModel}
      onUnloadModel={onUnloadModel}
      onShowModel={onShowModel}
      onShowLog={onShowLog}
      conversationId={1}
    />,
    { wrapper },
  );
}

/** The server registry says model `id` is up, that it was unloaded, or that it crashed. */
const event = (type: 'running' | 'stopped' | 'crashed', id: number) =>
  act(() => ingestServerEvent({ type, modelId: String(id), port: 4321, updatedAt: (clock += 1e6), modelName: 'qwen3' }));

/** Open the picker ▾ and return its item `name`. */
async function fromPicker(user: ReturnType<typeof userEvent.setup>, name: string): Promise<HTMLElement> {
  await user.click(modelPicker());
  return screen.getByRole('menuitem', { name });
}

/** The chat's head, once the saved turn is drawn and the thread has stopped remounting. */
async function head() {
  await screen.findByText('What does KeepAlive do?');
  return screen.getByRole('heading', { name: 'launchd KeepAlive' }).closest('[data-head]') as HTMLElement;
}

/** The line above the composer, by its words. */
const lineSaying = (words: string) => screen.findByText(words, { selector: '[role="status"] > span' });
const NO_LINE = /isn't loaded|stopped unexpectedly|endpoint is Off/;

beforeEach(() => {
  window.localStorage.clear();
  onUnloadModel.mockReset();
  onUnloadModel.mockImplementation(async () => {});
  onSwitchModel.mockClear();
  onShowModel.mockClear();
  onShowLog.mockClear();
  runFrames = [];
  started = [];
  const fixture = {
    conversations: [conversation(1, 'launchd KeepAlive')],
    rows: { 1: [{ id: 11, conversation_id: 1, role: 'user' as const, content: 'What does KeepAlive do?', created_at: '2026-09-01T09:12:00Z' }] },
    runs: [],
    frames: {},
  };
  transport.current = {
    ...chatTransport(fixture),
    // Nothing is loaded when the page reads the list.
    listServers: vi.fn(async () => []),
    startAgentRun: vi.fn(async (id: string, turn: LocalTurn) => {
      started.push(turn);
      return agentRun(id, 1, 'queued');
    }),
    readRunEvents: (_id: string, _after: number, signal: AbortSignal) => framesThenWait(runFrames, signal),
  };
  resetProxyState();
  applyProxyStatus(ON);
  forgetServerList();
  initServerEvents();
});

afterEach(() => {
  cleanupServerEvents();
  resetProxyState();
});

describe('ChatPlace, leaving and Unload', () => {
  it('has no Close: leaving the place (the sidebar navigates) unloads nothing', async () => {
    const page = renderLocal(7);
    expect(within(await head()).queryByRole('button', { name: 'Close' })).not.toBeInTheDocument();

    page.unmount();

    expect(onUnloadModel).not.toHaveBeenCalled();
  });

  it('a model unloaded mid-chat leaves the composer live, with one line that says a send loads it: no banner, no toast, no Load button', async () => {
    // An id of its own: the registry outlives the test.
    renderLocal(17);
    await head();
    event('running', 17);
    await waitFor(() => expect(screen.queryByText(NO_LINE)).not.toBeInTheDocument());

    event('stopped', 17);
    const line = await lineSaying("qwen3 isn't loaded · sending loads it");
    expect(line.closest('[role="status"]')).toHaveClass('text-xs');
    expect(screen.getByRole('textbox', { name: 'Message' })).toBeEnabled();
    expect(screen.getByRole('button', { name: 'Send' })).toBeInTheDocument();
    expect(modelPicker()).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Add to the message' })).toBeEnabled();
    expect(screen.queryByRole('button', { name: /Load it again|^Load/ })).not.toBeInTheDocument();
    expect(screen.queryByText(/was unloaded|Server not running|read-only/)).not.toBeInTheDocument();
    expect(screen.getByTestId('toasts')).toBeEmptyDOMElement();
    expect(onUnloadModel).not.toHaveBeenCalled();
  });

  it("a send with the model not loaded is sent, and the reply says the daemon's wait in its words", async () => {
    const user = userEvent.setup();
    renderLocal(20);
    await head();
    await lineSaying("qwen3 isn't loaded · sending loads it");
    runFrames = [{ type: 'waiting', reason: 'model_load', step: 0, total: 0, position: 0, model: 'qwen3', elapsed_ms: 12_300, said: 'Loading qwen3… 12 s' }];

    await user.type(screen.getByRole('textbox', { name: 'Message' }), 'Again?{Enter}');

    await waitFor(() => expect(started).toHaveLength(1));
    const wait = await screen.findByText('Loading qwen3… 12 s');
    expect(wait.closest('[data-turn="reply"]')).not.toBeNull();
    // While the reply waits, its own line says so: the one over the composer is gone.
    expect(screen.queryByText(NO_LINE)).not.toBeInTheDocument();
    expect(screen.getByTestId('toasts')).toBeEmptyDOMElement();
  });

  it('a model that crashed mid-chat says so, marked, with Show log, over a live composer', async () => {
    const user = userEvent.setup();
    renderLocal(18);
    await head();
    event('running', 18);

    event('crashed', 18);
    const line = (await lineSaying('qwen3 stopped unexpectedly · sending loads it again')).closest('[role="status"]') as HTMLElement;
    expect(line).toHaveTextContent('qwen3 stopped unexpectedly · sending loads it again·Show log');
    expect(screen.getByRole('textbox', { name: 'Message' })).toBeEnabled();
    expect(within(line).getAllByRole('button').map((b) => b.textContent)).toEqual(['Show log']);
    expect(screen.getByTestId('toasts')).toBeEmptyDOMElement();

    await user.click(within(line).getByRole('button', { name: 'Show log' }));
    expect(onShowLog).toHaveBeenCalledTimes(1);
  });

  it('with the endpoint off, the line says a send turns it on', async () => {
    renderLocal(21);
    await head();
    event('running', 21);
    act(() => applyProxyStatus(OFF));
    expect(await lineSaying('The endpoint is Off · sending turns it on')).toBeInTheDocument();
    expect(screen.getByRole('textbox', { name: 'Message' })).toBeEnabled();

    event('stopped', 21);
    expect(await lineSaying("qwen3 isn't loaded · sending turns the endpoint on and loads it")).toBeInTheDocument();

    act(() => applyProxyStatus(ON));
    expect(await lineSaying("qwen3 isn't loaded · sending loads it")).toBeInTheDocument();
  });

  it('says nothing before the server list and the endpoint have been read', async () => {
    cleanupServerEvents();
    forgetServerList();
    resetProxyState();
    renderLocal(22);
    await head();
    expect(screen.queryByText(NO_LINE)).not.toBeInTheDocument();
    expect(screen.getByRole('textbox', { name: 'Message' })).toBeEnabled();
  });

  it('Unload <model>, in the picker ▾ after a gap, hands up the unload and leaves the chat; Show <model> opens its inspector', async () => {
    const user = userEvent.setup();
    renderLocal(23);
    await head();
    event('running', 23);

    const unload = await fromPicker(user, 'Unload qwen3');
    expect(unload).toHaveAttribute('title', expect.stringMatching(/Copilot included/));
    expect(unload).toHaveClass('mt-sm');
    await user.click(unload);
    expect(onUnloadModel).toHaveBeenCalledTimes(1);
    expect(modelPicker()).toBeInTheDocument();

    await user.click(await fromPicker(user, 'Show qwen3'));
    expect(onShowModel).toHaveBeenCalledTimes(1);
  });

  it('offers no Unload while the model is not loaded', async () => {
    const user = userEvent.setup();
    renderLocal(24);
    await head();
    await lineSaying("qwen3 isn't loaded · sending loads it");
    await user.click(modelPicker());
    expect(screen.getByRole('menuitem', { name: 'Show qwen3' })).toBeInTheDocument();
    expect(screen.queryByRole('menuitem', { name: 'Unload qwen3' })).not.toBeInTheDocument();
  });

  it('Unload says why an unload failed', async () => {
    const user = userEvent.setup();
    onUnloadModel.mockRejectedValueOnce(new Error('daemon unreachable'));
    renderLocal(25);
    await head();
    event('running', 25);
    await user.click(await fromPicker(user, 'Unload qwen3'));

    await waitFor(() => expect(screen.getByTestId('toasts')).toHaveTextContent("Didn't unload: daemon unreachable"));
  });

  it('a chat with another machine offers no Unload and is never locked or lined by this machine’s registries (CH-9)', async () => {
    act(() => applyProxyStatus(OFF));
    render(
      <ChatPlace
        paired={{ far: { machine: { kind: 'paired', fingerprint: '3ca82708b995' }, id: 3 }, machineName: 'desk' }}
        modelName="qwen3"
        onSwitchModel={async () => {}}
        onUnloadModel={onUnloadModel}
        conversationId={1}
      />,
      { wrapper },
    );
    await head();
    await screen.findByText('qwen3 on desk');
    // The paired model's id is 3 there; here, model 3 crashed and the endpoint is off. Neither is the paired model's.
    event('crashed', 3);

    expect(screen.queryByRole('button', { name: /Unload/ })).not.toBeInTheDocument();
    expect(screen.queryByText(NO_LINE)).not.toBeInTheDocument();
    expect(screen.getByRole('textbox', { name: 'Message' })).toBeEnabled();
  });
});
