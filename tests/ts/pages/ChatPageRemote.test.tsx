/**
 * The chat screen when the model is on the other machine.
 *
 * Three things stop being true remotely and each one used to be assumed: the
 * page has a model id, it has a server port, and this window's server
 * registry knows how that model is doing. None hold across a tunnel — the
 * daemon supplies the port and the key per turn, and the registry only ever
 * hears about servers started here. Left as they were, the console would be
 * offered for a process this machine cannot see and the registry's silence
 * would be read as the server having stopped, locking the composer on a chat
 * that works perfectly.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, render, screen } from '@testing-library/react';
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

const getServerToolSupport = vi.fn(async () => ({
  supports_tool_calls: true,
  detected_format: null,
}));

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
      createConversation: vi.fn(async () => 1),
      getConversationMessages: vi.fn(async () => []),
      getSettings: vi.fn(async () => ({})),
      subscribe: vi.fn(() => () => {}),
    }),
  };
});

import ChatPage from '../../../src/pages/ChatPage';
import { ToastProvider, useToastContext } from '../../../src/contexts/ToastContext';
import { ConfirmProvider } from '../../../src/contexts/ConfirmContext';
import { SettingsProvider } from '../../../src/contexts/SettingsContext';
import { ingestServerEvent } from '../../../src/services/serverRegistry';

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

describe('ChatPage, remote', () => {
  beforeEach(() => {
    getServerToolSupport.mockClear();
  });

  it('offers no console and asks this machine nothing about the model', async () => {
    render(<ChatPage remote modelName="qwen3" onClose={async () => {}} />, { wrapper });

    expect(await screen.findByText('qwen3')).toBeInTheDocument();
    // The console reports a process on the other machine, which this window
    // has no port, id or log for.
    expect(screen.queryByRole('tab', { name: /console/i })).not.toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /chat/i })).toBeInTheDocument();
    // No model id to ask about, so the capability probe is never sent.
    expect(getServerToolSupport).not.toHaveBeenCalled();
  });

  it('does not read this machine\'s registry as the far machine dying', async () => {
    render(<ChatPage remote modelName="qwen3" onClose={async () => {}} />, { wrapper });
    await screen.findByText('qwen3');
    expect(screen.queryByText(/read-only/i)).not.toBeInTheDocument();

    // A remote session subscribes to `-1`, the id no model has. Nothing
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
