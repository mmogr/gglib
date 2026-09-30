/**
 * The composer's model picker: the model name in the composer margin, which
 * moves the chat to another model without leaving the conversation.
 *
 * The page does not move itself: it hands the choice and the open
 * conversation up, and `ModelControlCenterPage` remounts it on the new
 * session with that conversation. So the two halves are pinned here — what
 * the page hands up, and that a page given a conversation opens on it.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import type { ReactNode } from 'react';
import { chatTransport, conversation, wrapper as pageWrapper, type ChatFixture } from './chatPageHarness';
import { guiModel } from '../fixtures/model';

const transport = vi.hoisted(() => ({ current: {} as unknown }));
vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return { ...actual, getTransport: () => transport.current };
});

import ChatPage from '../../../src/pages/ChatPage';
import { ingestServerEvent } from '../../../src/services/serverRegistry';
import { useToastContext } from '../../../src/contexts/ToastContext';
import type { ModelChoice } from '../../../src/components/ChatMessagesPanel';

/** The toasts, which `ToastProvider` holds but does not draw. */
const ToastProbe = () => {
  const { toasts } = useToastContext();
  return <div data-testid="toasts">{toasts.map((t) => t.message).join(' | ')}</div>;
};
const wrapper = ({ children }: { children: ReactNode }) =>
  pageWrapper({ children: <><ToastProbe />{children}</> });

let fixture: ChatFixture;
type SwitchContext = () => { conversationId: number | null; draft: string };
const onSwitchModel = vi.fn(async (_choice: ModelChoice, _context: SwitchContext) => {});

function renderPage(conversationId?: number, modelId = 7, draft?: string) {
  return render(
    <ChatPage
      modelName="Qwen3.8-27B"
      modelId={modelId}
      draft={draft}
      serverPort={4321}
      conversationId={conversationId}
      onSwitchModel={onSwitchModel}
      onClose={async () => {}}
    />,
    { wrapper },
  );
}

/** Another model served here: running, so a switch to it starts nothing. */
function serve(modelId: number, modelName: string, type: 'running' | 'stopped') {
  act(() => ingestServerEvent({ type, modelId: String(modelId), port: 5555, updatedAt: Date.now(), modelName }));
}

beforeEach(() => {
  window.localStorage.clear();
  onSwitchModel.mockClear();
  fixture = {
    conversations: [conversation(1, 'launchd KeepAlive'), conversation(2, 'Parsing GGUF')],
    rows: {},
    runs: [],
    frames: {},
  };
  transport.current = {
    ...chatTransport(fixture),
    listModels: vi.fn(async () => [
      guiModel({ id: 7, name: 'Qwen3.8-27B' }),
      guiModel({ id: 8, name: 'llama-3.2-3b' }),
      guiModel({ id: 9, name: 'gemma-3-12b' }),
    ]),
  };
  serve(8, 'llama-3.2-3b', 'running');
});

afterEach(() => serve(8, 'llama-3.2-3b', 'stopped'));

describe('ChatPage, model picker', () => {
  it('lists the servers running here, then the models that are not', async () => {
    renderPage();
    // Queried afresh each time: the composer is remounted as the
    // conversation's messages load, and a held element goes stale.
    const picker = () => screen.getByRole('combobox', { name: 'Model' });
    const names = (group: string) =>
      within(within(picker()).getByRole('group', { name: group })).getAllByRole('option').map((o) => o.textContent);

    await waitFor(() => expect(names('Not running')).toEqual(['gemma-3-12b']));
    expect(names('Running')).toEqual(['llama-3.2-3b']);
    expect(picker()).toHaveDisplayValue('Qwen3.8-27B');
  });

  it('hands up the chosen model, and the conversation open when the switch lands', async () => {
    const user = userEvent.setup();
    renderPage();
    const selected = (name: RegExp) =>
      waitFor(() => expect(screen.getByRole('option', { name })).toHaveAttribute('aria-selected', 'true'));
    await user.click(await screen.findByRole('option', { name: /Parsing GGUF/ }));
    await selected(/Parsing GGUF/);

    await user.selectOptions(screen.getByRole('combobox', { name: 'Model' }), 'llama-3.2-3b');

    expect(onSwitchModel).toHaveBeenCalledTimes(1);
    const [choice, context] = onSwitchModel.mock.calls[0];
    expect(choice).toEqual({ modelId: 8, modelName: 'llama-3.2-3b' });
    expect(context().conversationId).toBe(2);
    // A model can take a while to start; a conversation opened meanwhile
    // is the one the new page opens on.
    await user.click(screen.getByRole('option', { name: /launchd KeepAlive/ }));
    await selected(/launchd KeepAlive/);
    expect(context().conversationId).toBe(1);
  });

  it('says why a model would not start, and stays on the model it was on', async () => {
    const user = userEvent.setup();
    onSwitchModel.mockRejectedValueOnce(new Error('not enough memory'));
    renderPage();
    const picker = () => screen.getByRole('combobox', { name: 'Model' });
    await waitFor(() => expect(within(picker()).getByRole('option', { name: 'gemma-3-12b' })).toBeInTheDocument());

    await user.selectOptions(picker(), 'gemma-3-12b');

    await waitFor(() =>
      expect(screen.getByTestId('toasts')).toHaveTextContent('Could not start gemma-3-12b: not enough memory'),
    );
    expect(picker()).toHaveDisplayValue('Qwen3.8-27B');
    expect(picker()).toBeEnabled();
  });

  it('lists the current model once, though it is running and registered', async () => {
    serve(10, 'Qwen3.8-27B', 'running');
    (transport.current as { listModels: () => Promise<unknown> }).listModels = async () => [
      guiModel({ id: 10, name: 'Qwen3.8-27B' }),
      guiModel({ id: 9, name: 'gemma-3-12b' }),
    ];
    try {
      renderPage(undefined, 10);
      const picker = () => screen.getByRole('combobox', { name: 'Model' });
      await waitFor(() => expect(within(picker()).getByRole('option', { name: 'gemma-3-12b' })).toBeInTheDocument());
      expect(within(picker()).getAllByRole('option', { name: 'Qwen3.8-27B' })).toHaveLength(1);
    } finally {
      serve(10, 'Qwen3.8-27B', 'stopped');
    }
  });

  it('opens on the conversation it is given, not the newest', async () => {
    renderPage(2);
    await waitFor(() =>
      expect(screen.getByRole('option', { name: /Parsing GGUF/ })).toHaveAttribute('aria-selected', 'true'),
    );
    expect(screen.getByRole('heading', { name: 'Parsing GGUF' })).toBeInTheDocument();
  });

  it('hands up the unsent text, and a page given it puts it back in the composer', async () => {
    const user = userEvent.setup();
    const said = { id: 11, conversation_id: 1, role: 'user' as const, content: 'What is a GGUF?', created_at: '2026-09-01T09:12:00Z' };
    fixture.rows = { 1: [said], 2: [{ ...said, id: 21, conversation_id: 2 }] };
    const page = renderPage();
    // Once the saved turn is drawn, the thread has stopped remounting.
    await screen.findByText('What is a GGUF?');
    await user.type(screen.getByRole('textbox', { name: 'Message' }), 'half a thought');
    await user.selectOptions(screen.getByRole('combobox', { name: 'Model' }), 'llama-3.2-3b');
    expect(onSwitchModel.mock.calls[0][1]().draft).toBe('half a thought');
    page.unmount();

    renderPage(2, 7, 'half a thought');
    await screen.findByText('What is a GGUF?');

    await waitFor(() => expect(screen.getByRole('textbox', { name: 'Message' })).toHaveValue('half a thought'));
  });
});
