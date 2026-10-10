/**
 * The chat screen when the model is the paired machine's.
 *
 * Three things stop being true there and each one used to be assumed: the
 * page has a model id here, it has a server port, and this window's server
 * registry knows how that model is doing. None hold across a tunnel — the
 * daemon supplies the port and the key per turn, and the registry only ever
 * hears about servers started here. Left as they were, the console would be
 * offered for a process this machine cannot see and the registry's silence
 * would be read as the server having stopped, locking the composer on a chat
 * that works perfectly.
 *
 * And the session's machine is fixed: the head names the model and its
 * machine, no picker offers this machine's models in its place, and a
 * conversation made here is made for the far model.
 *
 * Its Draw button is greyed: this machine is asked whether a message can
 * draw and told the model is the other machine's, and its reason is the
 * button's title.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom';
import { ReactNode } from 'react';

// jsdom has no ResizeObserver and assistant-ui's composer measures itself
// with one. Local to this file: it is the only test that mounts that tree.
class NoopResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= NoopResizeObserver as unknown as typeof ResizeObserver;

const createConversation = vi.fn(async (_params: object) => 2);
const getServerToolSupport = vi.fn(async () => ({
  supports_tool_calls: true,
  detected_format: null,
}));

/** As the daemon answers for a chat whose model is far (`drawing_availability`). */
const FAR_MODEL_REASON =
  "this chat's model is on another machine; a chat kept there draws with that machine's image model";
const drawingAvailability = vi.fn(async (_source: string, chat: { far?: boolean } = {}) =>
  chat.far
    ? { available: false, code: 'drawing_unavailable', reason: FAR_MODEL_REASON }
    : { available: true, model: 'flux' },
);

vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>(
    '../../../src/services/transport',
  );
  return {
    ...actual,
    getTransport: () => ({
      getServerToolSupport,
      listConversations: vi.fn(async () => [
        {
          id: 1,
          title: 'New Chat',
          model_id: null,
          system_prompt: null,
          settings: null,
          created_at: '2026-09-01T00:00:00Z',
          updated_at: '2026-09-01T00:00:00Z',
        },
      ]),
      createConversation,
      getThread: vi.fn(async () => ({ messages: [] })),
      listRuns: vi.fn(async () => []),
      getSettings: vi.fn(async () => ({})),
      drawingAvailability,
      subscribe: vi.fn(() => () => {}),
    }),
  };
});

import ChatPage from '../../../src/pages/ChatPage';
import { ToastProvider, useToastContext } from '../../../src/contexts/ToastContext';
import { ConfirmProvider } from '../../../src/contexts/ConfirmContext';
import { SettingsProvider } from '../../../src/contexts/SettingsContext';
import { ingestServerEvent } from '../../../src/services/serverRegistry';
import type { ModelRef } from '../../../src/types/generated/ModelRef';

const far: ModelRef = { machine: { kind: 'paired', fingerprint: '3ca82708b995' }, id: 3 };
const paired = { far, machineName: 'desk' };

/**
 * `ToastProvider` holds the toasts but renders none of them — the container
 * that does is mounted by `App`. Without this the queue is invisible to the
 * DOM, and an assertion that no toast was raised passes for the wrong reason.
 */
const ToastProbe = () => {
  const { toasts } = useToastContext();
  return <div data-testid="toasts">{toasts.map((t) => t.message).join(' | ')}</div>;
};

const wrapper = ({ children }: { children: ReactNode }) => (
  <ToastProvider>
    <ConfirmProvider>
      <SettingsProvider showToast={() => {}}>
        <ToastProbe />
        {children}
      </SettingsProvider>
    </ConfirmProvider>
  </ToastProvider>
);

describe('ChatPage, paired', () => {
  beforeEach(() => {
    getServerToolSupport.mockClear();
    createConversation.mockClear();
  });

  it('names the model and its machine, offers no console, and asks this machine nothing', async () => {
    render(<ChatPage paired={paired} modelName="qwen3" onClose={async () => {}} />, { wrapper });

    // The composer names the model and the machine it runs on.
    await waitFor(() => expect(screen.getByText('qwen3 on desk')).toBeInTheDocument());
    // The console reports a process on the other machine, which this window
    // has no port, id or log for.
    expect(screen.queryByRole('tab', { name: /console/i })).not.toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /chat/i })).toBeInTheDocument();
    // No model id here to ask about, so the capability probe is never sent.
    expect(getServerToolSupport).not.toHaveBeenCalled();
  });

  it('greys Draw with this machine\'s reason, asked by saying the model is the other machine\'s', async () => {
    render(<ChatPage paired={paired} modelName="qwen3" onClose={async () => {}} />, { wrapper });
    await screen.findByText('qwen3 on desk');

    const draw = await screen.findByRole('button', { name: 'Draw' });
    await waitFor(() => expect(draw).toHaveAttribute('title', FAR_MODEL_REASON));
    expect(draw).toBeDisabled();
    expect(draw).toHaveAttribute('aria-pressed', 'false');
    expect(drawingAvailability).toHaveBeenLastCalledWith('this', { far: true, callsTools: null });
  });

  it('offers no picker, since the session cannot move to this machine', async () => {
    render(<ChatPage paired={paired} modelName="qwen3" onClose={async () => {}} />, { wrapper });
    await screen.findByText('qwen3 on desk');
    expect(screen.queryByRole('combobox', { name: 'Model' })).not.toBeInTheDocument();
  });

  it('makes a new conversation for the far model', async () => {
    render(<ChatPage paired={paired} modelName="qwen3" onClose={async () => {}} />, { wrapper });
    await screen.findByText('qwen3 on desk');

    fireEvent.click(screen.getByRole('button', { name: 'New chat' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Create chat' }));

    await waitFor(() => expect(createConversation).toHaveBeenCalled());
    expect(createConversation.mock.calls.at(-1)?.[0]).toMatchObject({ model: far, modelId: null });
  });

  it('does not read this machine\'s registry as the far machine dying', async () => {
    render(<ChatPage paired={paired} modelName="qwen3" onClose={async () => {}} />, { wrapper });
    await screen.findByText('qwen3 on desk');
    expect(screen.queryByText(/read-only/i)).not.toBeInTheDocument();

    // A paired session subscribes to `-1`, the id no model has. Nothing
    // should ever publish under it — and if something does, the far machine
    // is still up and the composer must stay live rather than the chat going
    // read-only over a process on the wrong machine.
    act(() => {
      ingestServerEvent({
        type: 'crashed',
        modelId: '-1',
        port: 0,
        updatedAt: Date.now(),
        modelName: 'not the far machine',
      });
    });

    expect(screen.queryByText(/read-only/i)).not.toBeInTheDocument();
    expect(screen.getByTestId('toasts')).toHaveTextContent('');
  });
});
