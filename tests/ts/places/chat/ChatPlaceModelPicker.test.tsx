/**
 * The composer's model picker ▾: the model's name beside Send, a menu of
 * Loaded, On this Mac and the paired machine's models, which moves the chat
 * to another model without leaving the conversation.
 *
 * The page does not move itself: it hands the choice and the open
 * conversation up, and the chat route remounts it on the new
 * session with that conversation. So the two halves are pinned here — what
 * the page hands up, and that a page given a conversation opens on it.
 * Each of this machine's models says its state after its name, and a
 * pinned endpoint greys the models it will not start.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import type { ReactNode } from 'react';
import { chatTransport, conversation, modelPicker, pickModel, pickerGroup, wrapper as pageWrapper, type ChatFixture } from './chatPlaceHarness';
import { guiModel } from '../../fixtures/model';
import { applyLibraryRead, resetModelLibrary } from '../../../../src/services/modelRegistry';
import { IDLE_STATUS, applyRemoteStatus, resetRemoteState } from '../../../../src/services/remoteRegistry';
import { farEntry, pairedModels } from '../../fixtures/fakeFarDaemon';

const transport = vi.hoisted(() => ({ current: {} as unknown }));
vi.mock('../../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../../src/services/transport');
  return { ...actual, getTransport: () => transport.current };
});
// The proxy's live dashboard: the test sends its frames.
const dashboard = vi.hoisted(() => ({ send: null as ((snapshot: unknown) => void) | null }));
vi.mock('../../../../src/services/clients/proxyDashboard', () => ({
  subscribeProxyDashboard: (_h: string, _p: number, _k: unknown, onSnapshot: (s: unknown) => void) => {
    dashboard.send = onSnapshot;
    return () => {
      dashboard.send = null;
    };
  },
}));

import { ChatPlaceWithList as ChatPlace } from './chatPlaceHarness';
import { ingestServerEvent } from '../../../../src/services/serverRegistry';
import { applyProxyStatus, resetProxyState } from '../../../../src/services/proxyRegistry';
import { useToastContext } from '../../../../src/contexts/ToastContext';
import type { ModelChoice } from '../../../../src/components/ChatMessagesPanel';
import type { ChatDraft } from '../../../../src/types/messages';

/** The toasts, which `ToastProvider` holds but does not draw. */
const ToastProbe = () => {
  const { toasts } = useToastContext();
  return <div data-testid="toasts">{toasts.map((t) => t.message).join(' | ')}</div>;
};
const wrapper = ({ children }: { children: ReactNode }) =>
  pageWrapper({ children: <><ToastProbe />{children}</> });

let fixture: ChatFixture;
type SwitchContext = () => { conversationId: number | null; draft: ChatDraft };
const onSwitchModel = vi.fn(async (_choice: ModelChoice, _context: SwitchContext) => {});

function renderPage(conversationId?: number, modelId = 7, draft?: ChatDraft) {
  return render(
    <ChatPlace
      modelName="Qwen3.8-27B"
      modelId={modelId}
      draft={draft}
      serverPort={4321}
      conversationId={conversationId ?? fixture.conversations[0]?.id ?? null}
      onSwitchModel={onSwitchModel}
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
  transport.current = chatTransport(fixture);
  // The composer's model picker reads the registry, which App fills.
  applyLibraryRead({
    models: [
      guiModel({ id: 7, name: 'Qwen3.8-27B' }),
      guiModel({ id: 8, name: 'llama-3.2-3b' }),
      guiModel({ id: 9, name: 'gemma-3-12b' }),
    ],
  });
  serve(8, 'llama-3.2-3b', 'running');
});

afterEach(() => {
  act(() => resetProxyState());
  serve(8, 'llama-3.2-3b', 'stopped');
  resetModelLibrary();
  resetRemoteState();
});

describe('ChatPlace, model picker', () => {
  it('lists the models loaded here, the chat\'s own ticked, then the ones on this Mac that are not', async () => {
    const user = userEvent.setup();
    renderPage();
    // Once the conversation is drawn: the composer is remounted as its
    // messages load, and a menu opened before then is gone.
    await waitFor(() => expect(screen.getByRole('option', { name: /launchd KeepAlive/ })).toHaveAttribute('aria-selected', 'true'));
    await screen.findByRole('heading', { name: 'launchd KeepAlive' });
    await waitFor(() => expect(modelPicker()).toHaveTextContent('Qwen3.8-27B'));

    // The chat's own model, which nothing runs, is first under On this Mac, not under Loaded.
    expect(await pickerGroup(user, 'On this Mac')).toEqual(['Qwen3.8-27B · Not loaded', 'gemma-3-12b · Not loaded']);
    expect(await pickerGroup(user, 'Loaded')).toEqual(['llama-3.2-3b · Loaded']);
    expect(screen.getByRole('menuitemradio', { name: 'Qwen3.8-27B · Not loaded' })).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByRole('menuitemradio', { name: 'llama-3.2-3b · Loaded' })).toHaveAttribute('aria-checked', 'false');
  });

  it("says a running model's state as the endpoint's live stream gives it", async () => {
    const user = userEvent.setup();
    act(() => applyProxyStatus({ running: true, state: 'on', port: 9931, base_url: 'http://127.0.0.1:9931/v1', key: 'none_needed', pinned_model: null }));
    renderPage();
    await screen.findByRole('heading', { name: 'launchd KeepAlive' });
    await waitFor(() => expect(dashboard.send).not.toBeNull());
    act(() =>
      dashboard.send?.({
        admission: { slots: [{ model_id: 8, launch: { state: 'working', context_size: 8192, weights_bytes: 1, quantization: null, narration: null, flag_line: '' } }] },
      }),
    );
    expect(await pickerGroup(user, 'Loaded')).toEqual(['llama-3.2-3b · Working']);
  });

  it('while the endpoint is pinned, greys every model but the pinned one, the pin its reason', async () => {
    const user = userEvent.setup();
    act(() =>
      applyProxyStatus({ running: true, state: 'pinned', port: 9931, base_url: 'http://127.0.0.1:9931/v1', key: 'none_needed', pinned_model: 'llama-3.2-3b' }),
    );
    renderPage();
    await screen.findByRole('heading', { name: 'launchd KeepAlive' });
    await pickerGroup(user, 'Loaded');
    const gemma = screen.getByRole('menuitemradio', { name: 'gemma-3-12b · Not loaded' });
    expect(gemma).toBeDisabled();
    // Said under it and to a reader, in Rust's state word and the pinned name.
    expect(gemma).toHaveAccessibleDescription('Pinned · only llama-3.2-3b');
    const llama = screen.getByRole('menuitemradio', { name: 'llama-3.2-3b · Loaded' });
    expect(llama).toBeEnabled();
    expect(llama).not.toHaveAttribute('aria-describedby');
    // The chat's own model is never greyed by a pin: it is the one already open.
    const own = screen.getByRole('menuitemradio', { name: 'Qwen3.8-27B · Not loaded' });
    expect(own).toBeEnabled();
    expect(own).not.toHaveAttribute('aria-describedby');
    await user.click(screen.getByRole('menuitemradio', { name: 'gemma-3-12b · Not loaded' }));
    expect(onSwitchModel).not.toHaveBeenCalled();
  });

  it('draws no Loaded heading while nothing runs', async () => {
    const user = userEvent.setup();
    act(() => ingestServerEvent({ type: 'stopped', modelId: '8', port: 5555, updatedAt: Date.now() + 1, modelName: 'llama-3.2-3b' }));
    renderPage();
    await screen.findByRole('heading', { name: 'launchd KeepAlive' });
    expect(await pickerGroup(user, 'On this Mac')).toEqual(['Qwen3.8-27B · Not loaded', 'llama-3.2-3b · Not loaded', 'gemma-3-12b · Not loaded']);
    expect(screen.queryByRole('group', { name: 'Loaded' })).not.toBeInTheDocument();
  });

  it('greys nothing and names no pin while the endpoint serves any model', async () => {
    const user = userEvent.setup();
    act(() => applyProxyStatus({ running: true, state: 'on', port: 9931, base_url: 'http://127.0.0.1:9931/v1', key: 'none_needed', pinned_model: null }));
    renderPage();
    await screen.findByRole('heading', { name: 'launchd KeepAlive' });
    await pickerGroup(user, 'Loaded');
    expect(screen.getByRole('menuitemradio', { name: 'gemma-3-12b · Not loaded' })).toBeEnabled();
    expect(screen.getByRole('menuitemradio', { name: 'gemma-3-12b · Not loaded' })).not.toHaveAttribute('aria-describedby');
    expect(screen.queryByText(/^Pinned/)).not.toBeInTheDocument();
  });

  it('hands up the chosen model, and the conversation open when the switch lands', async () => {
    const user = userEvent.setup();
    renderPage();
    const selected = (name: RegExp) =>
      waitFor(() => expect(screen.getByRole('option', { name })).toHaveAttribute('aria-selected', 'true'));
    await user.click(await screen.findByRole('option', { name: /Parsing GGUF/ }));
    await selected(/Parsing GGUF/);

    await pickModel(user, 'llama-3.2-3b · Loaded');

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
    // The composer is remounted as the conversation's messages load, so the
    // pick waits for one to be open, as the test above does.
    await user.click(await screen.findByRole('option', { name: /Parsing GGUF/ }));
    await waitFor(() => expect(screen.getByRole('option', { name: /Parsing GGUF/ })).toHaveAttribute('aria-selected', 'true'));
    await screen.findByRole('heading', { name: 'Parsing GGUF' });

    await pickModel(user, 'gemma-3-12b · Not loaded');

    // The inspector's sentence, the daemon's reason as it came.
    await waitFor(() => expect(screen.getByTestId('toasts')).toHaveTextContent("Didn't load: not enough memory"));
    expect(modelPicker()).toHaveTextContent('Qwen3.8-27B');
    expect(modelPicker()).toBeEnabled();
  });

  it('lists the current model once, though it is running and registered', async () => {
    serve(10, 'Qwen3.8-27B', 'running');
    applyLibraryRead({ models: [guiModel({ id: 10, name: 'Qwen3.8-27B' }), guiModel({ id: 9, name: 'gemma-3-12b' })] });
    try {
      const user = userEvent.setup();
      renderPage(undefined, 10);
      await screen.findByRole('heading', { name: 'launchd KeepAlive' });
      expect(await pickerGroup(user, 'On this Mac')).toEqual(['gemma-3-12b · Not loaded']);
      expect(screen.getAllByRole('menuitemradio', { name: 'Qwen3.8-27B · Loaded' })).toHaveLength(1);
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
    await pickModel(user, 'llama-3.2-3b · Loaded');
    expect(onSwitchModel.mock.calls[0][1]().draft).toEqual({ text: 'half a thought', images: [] });
    page.unmount();

    renderPage(2, 7, { text: 'half a thought', images: [] });
    await screen.findByText('What is a GGUF?');

    await waitFor(() => expect(screen.getByRole('textbox', { name: 'Message' })).toHaveValue('half a thought'));
  });

  describe("the paired machine's models", () => {
    const onOpenFar = vi.fn();
    function joinDesk() {
      (transport.current as Record<string, unknown>).listPairedModels = vi.fn(async () => pairedModels([farEntry('Llama-3.3-70B', 42)]));
      act(() =>
        applyRemoteStatus({
          ...IDLE_STATUS,
          connected: { port: 41234, base_url: 'http://127.0.0.1:41234/v1', ticket_fingerprint: '3ca82708b995', path: 'direct', away_for_s: null },
          paired_name: 'desk',
        }),
      );
    }
    const renderOn = (conversationId: number | null) =>
      render(
        <ChatPlace
          modelName="Qwen3.8-27B"
          modelId={7}
          serverPort={4321}
          conversationId={conversationId}
          onSwitchModel={onSwitchModel}
          onOpenFar={onOpenFar}
        />,
        { wrapper },
      );

    beforeEach(() => onOpenFar.mockClear());

    it('are a group of their own in a new chat, and one opens a new chat on it', async () => {
      const user = userEvent.setup();
      joinDesk();
      renderOn(null);
      await screen.findByRole('textbox', { name: 'Message' });
      await waitFor(async () => expect(await pickerGroup(user, 'On desk')).toEqual(['Llama-3.3-70B']));
      const desk = screen.getByRole('menuitem', { name: 'Llama-3.3-70B' });
      expect(desk).toBeEnabled();
      expect(desk).toHaveAttribute('title', 'Opens a new chat on Llama-3.3-70B, on desk.');
      await user.click(desk);
      expect(onOpenFar).toHaveBeenCalledWith(42);
      expect(onSwitchModel).not.toHaveBeenCalled();
    });

    it('are listed greyed in a saved chat, which stays on its machine, saying why under each', async () => {
      const user = userEvent.setup();
      joinDesk();
      renderOn(1);
      await screen.findByRole('heading', { name: 'launchd KeepAlive' });
      await waitFor(async () => expect(await pickerGroup(user, 'On desk')).toEqual(['Llama-3.3-70B']));
      const desk = screen.getByRole('menuitem', { name: 'Llama-3.3-70B' });
      expect(desk).toBeDisabled();
      expect(desk).toHaveAccessibleDescription('A chat stays on its machine. Start a new chat to use this model.');
      expect(onOpenFar).not.toHaveBeenCalled();
    });

    it('are never greyed by this machine\'s pinned endpoint, which does not serve them', async () => {
      const user = userEvent.setup();
      joinDesk();
      act(() =>
        applyProxyStatus({ running: true, state: 'pinned', port: 9931, base_url: 'http://127.0.0.1:9931/v1', key: 'none_needed', pinned_model: 'llama-3.2-3b' }),
      );
      renderOn(null);
      await screen.findByRole('textbox', { name: 'Message' });
      await waitFor(async () => expect(await pickerGroup(user, 'On desk')).toEqual(['Llama-3.3-70B']));
      expect(screen.getByRole('menuitemradio', { name: 'gemma-3-12b · Not loaded' })).toBeDisabled();
      expect(screen.getByRole('menuitem', { name: 'Llama-3.3-70B' })).toBeEnabled();
    });

    it('are not there with no machine joined', async () => {
      const user = userEvent.setup();
      renderOn(null);
      await screen.findByRole('textbox', { name: 'Message' });
      await pickerGroup(user, 'Loaded');
      expect(screen.queryByRole('group', { name: 'On desk' })).not.toBeInTheDocument();
    });
  });

  it('stays locked while a model starts, across a change of conversation', async () => {
    const user = userEvent.setup();
    render(
      <ChatPlace
        modelName="Qwen3.8-27B"
        modelId={7}
        serverPort={4321}
        startingModel="gemma-3-12b"
        onSwitchModel={onSwitchModel}
        conversationId={fixture.conversations[0]?.id ?? null}
      />,
      { wrapper },
    );
    const locked = async () => {
      await waitFor(() => expect(screen.getByText('Loading gemma-3-12b…')).toHaveAttribute('role', 'status'));
      expect(screen.queryByRole('button', { name: /^Model: / })).not.toBeInTheDocument();
    };
    await locked();

    // The composer, picker and all, remounts with the conversation.
    await user.click(await screen.findByRole('option', { name: /Parsing GGUF/ }));
    await waitFor(() =>
      expect(screen.getByRole('option', { name: /Parsing GGUF/ })).toHaveAttribute('aria-selected', 'true'),
    );
    await locked();
  });
});
