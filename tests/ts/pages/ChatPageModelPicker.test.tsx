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
import { chatTransport, conversation, wrapper, type ChatFixture } from './chatPageHarness';
import { guiModel } from '../fixtures/model';

const transport = vi.hoisted(() => ({ current: {} as unknown }));
vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return { ...actual, getTransport: () => transport.current };
});

import ChatPage from '../../../src/pages/ChatPage';
import { ingestServerEvent } from '../../../src/services/serverRegistry';

let fixture: ChatFixture;
const onSwitchModel = vi.fn(async () => {});

function renderPage(conversationId?: number) {
  return render(
    <ChatPage
      modelName="Qwen3.8-27B"
      modelId={7}
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

  it('hands up the chosen model with the conversation that is open', async () => {
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole('option', { name: /Parsing GGUF/ }));
    await waitFor(() =>
      expect(screen.getByRole('option', { name: /Parsing GGUF/ })).toHaveAttribute('aria-selected', 'true'),
    );

    await user.selectOptions(screen.getByRole('combobox', { name: 'Model' }), 'llama-3.2-3b');

    expect(onSwitchModel).toHaveBeenCalledWith({ modelId: 8, modelName: 'llama-3.2-3b' }, 2);
  });

  it('opens on the conversation it is given, not the newest', async () => {
    renderPage(2);
    await waitFor(() =>
      expect(screen.getByRole('option', { name: /Parsing GGUF/ })).toHaveAttribute('aria-selected', 'true'),
    );
    expect(screen.getByRole('heading', { name: 'Parsing GGUF' })).toBeInTheDocument();
  });
});
