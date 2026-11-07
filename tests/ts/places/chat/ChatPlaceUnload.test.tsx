/**
 * Leaving a chat and unloading its model are two different wishes (#1211).
 *
 * The proxy serves the chat's model to every client, Copilot included, so
 * leaving the chat (the sidebar navigates; there is no Close) stops nothing,
 * and Unload, beside the model picker, is the one control that stops the
 * model (the model's inspector has its own). The place hands it up to the
 * chat route, which `tests/ts/places/libraryChat.test.tsx` pins (CH-10).
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import type { ReactNode } from 'react';
import { chatTransport, conversation, wrapper as pageWrapper } from './chatPlaceHarness';

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

function renderLocal(modelId = 7, startingModel: string | null = null) {
  return render(
    <ChatPlace
      modelName="qwen3"
      modelId={modelId}
      serverPort={4321}
      startingModel={startingModel}
      onSwitchModel={async () => {}}
      onUnloadModel={onUnloadModel}
      conversationId={1}
    />,
    { wrapper },
  );
}

/** The notebook head, once the saved turn is drawn and the thread has stopped remounting. */
async function head() {
  await screen.findByText('What does KeepAlive do?');
  return screen.getByRole('heading', { name: 'launchd KeepAlive' }).closest('.grid') as HTMLElement;
}

beforeEach(() => {
  window.localStorage.clear();
  onUnloadModel.mockReset();
  onUnloadModel.mockImplementation(async () => {});
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

  it('the read-only banner offers no Close and unloads nothing, and Unload goes', async () => {
    // An id of its own: the registry outlives the test.
    renderLocal(17);
    await head();
    expect(await screen.findByRole('button', { name: 'Unload' })).toBeInTheDocument();

    act(() => ingestServerEvent({ type: 'stopped', modelId: '17', port: 4321, updatedAt: Date.now(), modelName: 'qwen3' }));
    const banner = (await screen.findByText(/Server not running/)).closest('[role="status"]') as HTMLElement;
    expect(within(banner).queryByRole('button')).not.toBeInTheDocument();

    expect(onUnloadModel).not.toHaveBeenCalled();
    // Nothing is left to unload.
    expect(screen.queryByRole('button', { name: 'Unload' })).not.toBeInTheDocument();
  });

  it('Unload beside the picker hands up the unload and leaves the chat', async () => {
    const user = userEvent.setup();
    renderLocal();
    await head();

    const unload = await screen.findByRole('button', { name: 'Unload' });
    expect(unload).toHaveAttribute('title', expect.stringMatching(/Copilot included/));
    await user.click(unload);

    expect(onUnloadModel).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('combobox', { name: 'Model' })).toBeInTheDocument();
  });

  it('Unload is locked while a model starts, and says why an unload failed', async () => {
    const user = userEvent.setup();
    const page = renderLocal(7, 'gemma-3-12b');
    await head();
    expect(await screen.findByRole('button', { name: 'Unload' })).toBeDisabled();
    page.unmount();

    onUnloadModel.mockRejectedValueOnce(new Error('daemon unreachable'));
    renderLocal();
    await head();
    await user.click(await screen.findByRole('button', { name: 'Unload' }));

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
