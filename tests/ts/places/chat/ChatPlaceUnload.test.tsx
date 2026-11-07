/**
 * Leaving a chat and unloading its model are two different wishes (#1211).
 *
 * The proxy serves the chat's model to every client, Copilot included, so
 * leaving the chat (the sidebar navigates; there is no Close) stops nothing,
 * and Unload <model>, in the model picker ▾, is the one control here that
 * stops the model (the model's inspector has its own). The place hands it
 * up to the chat route, which `tests/ts/places/libraryChat.test.tsx` pins
 * (CH-10). A model gone mid-chat replaces the composer with one line that
 * says so, with no banner and no toast: "was unloaded. [Load it again]", or
 * "stopped unexpectedly · Show log".
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import type { ReactNode } from 'react';
import { chatTransport, conversation, modelPicker, wrapper as pageWrapper } from './chatPlaceHarness';

const transport = vi.hoisted(() => ({ current: {} as unknown }));
vi.mock('../../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../../src/services/transport');
  return { ...actual, getTransport: () => transport.current };
});

import ChatPlace from '../../../../src/places/chat/ChatPlace';
import { ingestServerEvent } from '../../../../src/services/serverRegistry';
import { useToastContext } from '../../../../src/contexts/ToastContext';

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

function renderLocal(modelId = 7, startingModel: string | null = null) {
  return render(
    <ChatPlace
      modelName="qwen3"
      modelId={modelId}
      serverPort={4321}
      startingModel={startingModel}
      onSwitchModel={onSwitchModel}
      onUnloadModel={onUnloadModel}
      onShowModel={onShowModel}
      onShowLog={onShowLog}
      conversationId={1}
    />,
    { wrapper },
  );
}

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

beforeEach(() => {
  window.localStorage.clear();
  onUnloadModel.mockReset();
  onUnloadModel.mockImplementation(async () => {});
  onSwitchModel.mockClear();
  onShowModel.mockClear();
  onShowLog.mockClear();
  transport.current = chatTransport({
    conversations: [conversation(1, 'launchd KeepAlive')],
    rows: { 1: [{ id: 11, conversation_id: 1, role: 'user', content: 'What does KeepAlive do?', created_at: '2026-09-01T09:12:00Z' }] },
    runs: [],
    frames: {},
  });
});

describe('ChatPlace, leaving and Unload', () => {
  it('has no Close: leaving the place (the sidebar navigates) unloads nothing', async () => {
    const page = renderLocal();
    expect(within(await head()).queryByRole('button', { name: 'Close' })).not.toBeInTheDocument();

    page.unmount();

    expect(onUnloadModel).not.toHaveBeenCalled();
  });

  it('a model unloaded mid-chat replaces the composer with "was unloaded. [Load it again]": no banner, no toast, no Close', async () => {
    const user = userEvent.setup();
    // An id of its own: the registry outlives the test.
    renderLocal(17);
    await head();
    expect(screen.getByRole('textbox', { name: 'Message' })).toBeInTheDocument();

    act(() => ingestServerEvent({ type: 'stopped', modelId: '17', port: 4321, updatedAt: Date.now(), modelName: 'qwen3' }));
    const line = await screen.findByText('qwen3 was unloaded.');
    expect(line).toHaveAttribute('role', 'status');
    expect(screen.queryByRole('textbox', { name: 'Message' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Model: / })).not.toBeInTheDocument();
    expect(screen.queryByText(/Server not running|read-only/)).not.toBeInTheDocument();
    expect(screen.getByTestId('toasts')).toBeEmptyDOMElement();
    expect(within(line).getAllByRole('button').map((b) => b.textContent)).toEqual(['Load it again']);
    expect(onUnloadModel).not.toHaveBeenCalled();

    // Load it again is the picker's switch, back onto the same model.
    await user.click(within(line).getByRole('button', { name: 'Load it again' }));
    expect(onSwitchModel).toHaveBeenCalledTimes(1);
    expect(onSwitchModel.mock.calls[0][0]).toEqual({ modelId: 17, modelName: 'qwen3' });
  });

  it('a model that crashed mid-chat says "stopped unexpectedly · Show log", and Show log is handed up', async () => {
    const user = userEvent.setup();
    renderLocal(18);
    await head();

    act(() => ingestServerEvent({ type: 'crashed', modelId: '18', port: 4321, updatedAt: Date.now(), modelName: 'qwen3' }));
    const line = (await screen.findByText(/^qwen3 stopped unexpectedly/)).closest('[role="status"]') as HTMLElement;
    expect(line).toHaveTextContent('qwen3 stopped unexpectedly·Show log');
    expect(screen.queryByRole('textbox', { name: 'Message' })).not.toBeInTheDocument();
    expect(within(line).queryByRole('button', { name: 'Load it again' })).not.toBeInTheDocument();
    expect(screen.getByTestId('toasts')).toBeEmptyDOMElement();

    await user.click(within(line).getByRole('button', { name: 'Show log' }));
    expect(onShowLog).toHaveBeenCalledTimes(1);
  });

  it('while Load it again starts the model, the line says it is loading', async () => {
    renderLocal(19, 'qwen3');
    await head();
    act(() => ingestServerEvent({ type: 'stopped', modelId: '19', port: 4321, updatedAt: Date.now(), modelName: 'qwen3' }));
    expect(await screen.findByText('Loading qwen3…')).toHaveAttribute('role', 'status');
    expect(screen.queryByRole('button', { name: 'Load it again' })).not.toBeInTheDocument();
  });

  it('Unload <model>, in the picker ▾ after a gap, hands up the unload and leaves the chat; Show <model> opens its inspector', async () => {
    const user = userEvent.setup();
    renderLocal();
    await head();

    const unload = await fromPicker(user, 'Unload qwen3');
    expect(unload).toHaveAttribute('title', expect.stringMatching(/Copilot included/));
    expect(unload).toHaveClass('mt-sm');
    await user.click(unload);
    expect(onUnloadModel).toHaveBeenCalledTimes(1);
    expect(modelPicker()).toBeInTheDocument();

    await user.click(await fromPicker(user, 'Show qwen3'));
    expect(onShowModel).toHaveBeenCalledTimes(1);
  });

  it('Unload is out of reach while a model starts, and says why an unload failed', async () => {
    const user = userEvent.setup();
    const page = renderLocal(7, 'gemma-3-12b');
    await head();
    expect(await screen.findByText('Loading gemma-3-12b…')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Model: / })).not.toBeInTheDocument();
    page.unmount();

    onUnloadModel.mockRejectedValueOnce(new Error('daemon unreachable'));
    renderLocal();
    await head();
    await user.click(await fromPicker(user, 'Unload qwen3'));

    await waitFor(() => expect(screen.getByTestId('toasts')).toHaveTextContent('Could not unload qwen3: daemon unreachable'));
  });

  it('a chat with another machine offers no Unload: the model is not this one’s', async () => {
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

    expect(screen.queryByRole('button', { name: /Unload/ })).not.toBeInTheDocument();
  });
});
