/**
 * Leaving a chat and unloading its model are two different wishes (#1211).
 *
 * The proxy serves the chat's model to every client, Copilot included, so
 * Close only leaves the chat, and Unload, beside the model picker and in the
 * Console, is the one control that stops the model. The page hands each up
 * to `ModelControlCenterPage`, whose test pins what it does with them; this
 * one pins which control hands up which.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import type { ReactNode } from 'react';
import { chatTransport, conversation, wrapper as pageWrapper } from './chatPageHarness';

const transport = vi.hoisted(() => ({ current: {} as unknown }));
vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return { ...actual, getTransport: () => transport.current };
});
// The Console's log is the daemon's; no stream opens.
vi.mock('../../../src/hooks/useServerLogs', () => ({
  useServerLogs: () => ({ logs: [], clearLogs: () => {}, isAutoScroll: true, setIsAutoScroll: () => {}, copyAllLogs: () => {} }),
}));

import ChatPage from '../../../src/pages/ChatPage';
import { ingestServerEvent } from '../../../src/services/serverRegistry';
import { useToastContext } from '../../../src/contexts/ToastContext';

/** The toasts, which `ToastProvider` holds but does not draw. */
const ToastProbe = () => {
  const { toasts } = useToastContext();
  return <div data-testid="toasts">{toasts.map((t) => t.message).join(' | ')}</div>;
};
const wrapper = ({ children }: { children: ReactNode }) =>
  pageWrapper({ children: <><ToastProbe />{children}</> });

const onClose = vi.fn(() => {});
const onUnloadModel = vi.fn(async () => {});

function renderLocal(modelId = 7, startingModel: string | null = null) {
  return render(
    <ChatPage
      modelName="qwen3"
      modelId={modelId}
      serverPort={4321}
      startingModel={startingModel}
      onSwitchModel={async () => {}}
      onUnloadModel={onUnloadModel}
      onClose={onClose}
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
  onClose.mockClear();
  onUnloadModel.mockReset();
  onUnloadModel.mockImplementation(async () => {});
  transport.current = chatTransport({
    conversations: [conversation(1, 'launchd KeepAlive')],
    rows: { 1: [{ id: 11, conversation_id: 1, role: 'user', content: 'What does KeepAlive do?', created_at: '2026-09-01T09:12:00Z' }] },
    runs: [],
    frames: {},
  });
});

describe('ChatPage, Close and Unload', () => {
  it('Close leaves the chat, unloads nothing, and does not say it stops anything', async () => {
    const user = userEvent.setup();
    renderLocal();
    const close = within(await head()).getByRole('button', { name: 'Close' });

    expect(close).toHaveAttribute('title', 'Close chat');
    await user.click(close);

    expect(onClose).toHaveBeenCalledTimes(1);
    expect(onUnloadModel).not.toHaveBeenCalled();
  });

  it("the read-only banner's Close leaves the chat and unloads nothing, and Unload goes", async () => {
    const user = userEvent.setup();
    // An id of its own: the registry outlives the test.
    renderLocal(17);
    await head();
    expect(await screen.findByRole('button', { name: 'Unload' })).toBeInTheDocument();

    act(() => ingestServerEvent({ type: 'stopped', modelId: '17', port: 4321, updatedAt: Date.now(), modelName: 'qwen3' }));
    const banner = (await screen.findByText(/Server not running/)).closest('[role="status"]') as HTMLElement;
    await user.click(within(banner).getByRole('button', { name: 'Close' }));

    expect(onClose).toHaveBeenCalledTimes(1);
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
    expect(onClose).not.toHaveBeenCalled();
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

  it("the Console's Unload model hands up the unload, not Close", async () => {
    const user = userEvent.setup();
    renderLocal();
    await user.click(within(await head()).getByRole('tab', { name: /console/i }));

    await user.click(screen.getByRole('button', { name: 'Unload model' }));

    expect(onUnloadModel).toHaveBeenCalledTimes(1);
    expect(onClose).not.toHaveBeenCalled();
  });

  it('a chat with another machine offers no Unload: the model is not this one’s', async () => {
    render(
      <ChatPage remote modelName="qwen3" onSwitchModel={async () => {}} onUnloadModel={onUnloadModel} onClose={onClose} />,
      { wrapper },
    );
    await head();
    await screen.findByText('qwen3');

    expect(screen.queryByRole('button', { name: /Unload/ })).not.toBeInTheDocument();
  });
});
