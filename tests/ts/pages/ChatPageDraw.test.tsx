/**
 * Drawing on the chat page: the composer's Draw button, and a render as its
 * reply shows it.
 *
 * The button is greyed, its title the reason, where the machine that would
 * draw answers that it cannot, and a send from there says nothing of
 * drawing. Where it can, a press arms it for one message: the run that
 * message starts says `draw`, the button is not pressed once that run is
 * accepted, and the next message's run has no such key. A run that was
 * refused leaves it pressed.
 *
 * While the image is made, the tool's row says how far it has got in words
 * and a bar, and shows the picture so far, larger than it came; the frame is
 * gone once the tool's result is drawn. A reply that cannot start for a
 * render says what it is queued behind and the step that render is on.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import type { AgentRunRequest } from '../../../src/types/generated/AgentRunRequest';
import type { DrawingAvailability } from '../../../src/types/generated/DrawingAvailability';
import type { RunInfo } from '../../../src/types/generated/RunInfo';
import { NO_DRAWING, agentRun, chatTransport, conversation, wrapper, type ChatFixture } from './chatPageHarness';

const transport = vi.hoisted(() => ({ current: {} as unknown }));
vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return { ...actual, getTransport: () => transport.current };
});

import ChatPage from '../../../src/pages/ChatPage';

type User = ReturnType<typeof userEvent.setup>;
type Item =
  | { type: 'frame'; seq: number; data: string }
  | { type: 'preview'; toolCallId: string; data: string }
  | { type: 'end'; info: RunInfo };

const CAN: DrawingAvailability = { available: true, model: 'flux-schnell' };
const PNG = 'iVBORw0KGgoAAAANSUhEUgAAAIAAAACA';

let fixture: ChatFixture;
/** What this machine answers when asked whether a message can draw. */
let availability: DrawingAvailability;
/** The bodies of the runs started, in order. */
let started: AgentRunRequest[];
/** The next run start is refused with this, once. */
let refuseNext: Error | null;
/** Hands an item to the reader of the run going, and ends that run. */
let feed: (item: Item) => void;
let seq: number;

function pageTransport() {
  return {
    ...chatTransport(fixture),
    drawingAvailability: vi.fn(async () => availability),
    startAgentRun: vi.fn(async (id: string, request: AgentRunRequest) => {
      if (refuseNext) {
        const refusal = refuseNext;
        refuseNext = null;
        throw refusal;
      }
      started.push(request);
      return agentRun(id, 1, 'queued');
    }),
    // A run read as the test feeds it, until it ends or its reader leaves.
    readRunEvents: async function* (_id: string, _after: number, signal: AbortSignal) {
      const queue: Item[] = [];
      let wake = () => {};
      feed = (item) => {
        queue.push(item);
        wake();
      };
      signal.addEventListener('abort', () => wake());
      while (!signal.aborted) {
        while (queue.length > 0) {
          const item = queue.shift()!;
          yield item;
          if (item.type === 'end') return;
        }
        await new Promise<void>((resolve) => (wake = resolve));
      }
    },
  };
}

const stub = () => transport.current as ReturnType<typeof pageTransport>;
const drawButton = () => screen.getByRole('button', { name: 'Draw' });

/** One logged event of the run going. */
function emit(event: object) {
  feed({ type: 'frame', seq: ++seq, data: JSON.stringify(event) });
}

/** One preview frame of the run going, beside its log. */
function preview(toolCallId: string, step: number) {
  const data = JSON.stringify({ tool_call_id: toolCallId, frame: { mime: 'image/png', step, total: 20, b64: `${PNG}${step}` } });
  feed({ type: 'preview', toolCallId, data });
}

/** End the run going with `reply` saved. */
function end(reply: string) {
  fixture.rows[1] = [
    ...(fixture.rows[1] ?? []),
    { id: 90 + seq, conversation_id: 1, role: 'assistant', content: reply, created_at: '2026-10-10T10:00:00Z' },
  ];
  feed({ type: 'end', info: agentRun('r', 1, 'completed') });
}

async function renderPage() {
  render(<ChatPage modelName="Qwen3.8-27B" modelId={7} serverPort={4321} onClose={async () => {}} />, { wrapper });
  await waitFor(() => expect(screen.queryByText('Loading messages…')).not.toBeInTheDocument());
  await waitFor(() => expect(drawButton()).not.toHaveAttribute('title', 'Checking whether a message here can draw.'));
}

/** Send `text` and wait for its run to be started, or refused. */
async function ask(user: User, text: string) {
  const calls = stub().startAgentRun.mock.calls.length;
  await user.type(screen.getByRole('textbox', { name: 'Message' }), `${text}{Enter}`);
  await waitFor(() => expect(stub().startAgentRun).toHaveBeenCalledTimes(calls + 1));
}

const toolStart = {
  type: 'tool_call_start',
  tool_call: { id: 'call_1', name: 'builtin:generate_image', arguments: { prompt: 'a lighthouse at dusk' } },
  display_name: 'Generate Image',
};
const toolDone = {
  type: 'tool_call_complete',
  tool_name: 'builtin:generate_image',
  result: { tool_call_id: 'call_1', content: 'Drew 1 image.', success: true },
  wait_ms: 0,
  execute_duration_ms: 76000,
  display_name: 'Generate Image',
  duration_display: '76s',
};

beforeEach(() => {
  window.localStorage.clear();
  fixture = { conversations: [conversation(1, 'Lighthouses')], rows: {}, runs: [], frames: {} };
  availability = CAN;
  started = [];
  refuseNext = null;
  seq = 0;
  feed = () => {};
  transport.current = pageTransport();
});

describe('ChatPage, the Draw button', () => {
  it('is greyed, its title the machine\'s reason, where a message cannot draw, and its send says nothing of drawing', async () => {
    const user = userEvent.setup();
    availability = NO_DRAWING;
    await renderPage();

    expect(drawButton()).toBeDisabled();
    expect(drawButton()).toHaveAttribute('title', NO_DRAWING.reason);
    expect(drawButton()).toHaveAttribute('aria-pressed', 'false');
    await user.click(drawButton());
    expect(drawButton()).toHaveAttribute('aria-pressed', 'false');

    await ask(user, 'draw a lighthouse');
    expect(Object.keys(started[0])).not.toContain('draw');
  });

  it('is not pressed until clicked, and says which model would draw', async () => {
    await renderPage();

    expect(drawButton()).toBeEnabled();
    expect(drawButton()).toHaveAttribute('aria-pressed', 'false');
    expect(drawButton()).toHaveTextContent(/^Draw$/);
    expect(drawButton()).toHaveAttribute(
      'title',
      'Let the next message draw an image with flux-schnell. It applies to that message only.',
    );
  });

  it('pressed, it reads so without its colour, and a second press takes it back', async () => {
    const user = userEvent.setup();
    await renderPage();

    await user.click(drawButton());
    expect(drawButton()).toHaveAttribute('aria-pressed', 'true');
    expect(drawButton()).toHaveTextContent('Draw on');
    expect(drawButton()).toHaveAttribute('title', 'The next message draws an image. Click to send it without drawing.');

    await user.click(drawButton());
    expect(drawButton()).toHaveAttribute('aria-pressed', 'false');
    await ask(user, 'no picture, thanks');
    expect(Object.keys(started[0])).not.toContain('draw');
  });

  it('the message sent while pressed says draw; the button resets once its run is accepted, and the next says nothing', async () => {
    const user = userEvent.setup();
    await renderPage();
    await user.click(drawButton());

    await ask(user, 'draw a lighthouse');
    expect(started[0].draw).toBe(true);
    // Reset while the reply is still being written: it was for that message alone.
    await waitFor(() => expect(drawButton()).toHaveAttribute('aria-pressed', 'false'));
    expect(screen.getByRole('button', { name: 'Stop' })).toBeInTheDocument();

    end('A lighthouse at dusk.');
    await screen.findByText('A lighthouse at dusk.');
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Stop' })).not.toBeInTheDocument());

    await ask(user, 'thanks');
    expect(Object.keys(started[1])).not.toContain('draw');
    expect(drawButton()).toHaveAttribute('aria-pressed', 'false');
  });

  it('stays pressed when the run that said draw is refused, and the message sent again says it again', async () => {
    const user = userEvent.setup();
    await renderPage();
    await user.click(drawButton());

    refuseNext = new Error('no image model is installed; download one first');
    await ask(user, 'draw a lighthouse');
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Stop' })).not.toBeInTheDocument());
    expect(started).toEqual([]);
    expect(drawButton()).toHaveAttribute('aria-pressed', 'true');

    await ask(user, '{Enter}');
    expect(started[0].draw).toBe(true);
    await waitFor(() => expect(drawButton()).toHaveAttribute('aria-pressed', 'false'));
  });
});

describe('ChatPage, a render in its reply', () => {
  /** A message sent with Draw pressed, whose run has called the drawing tool. */
  async function drawing() {
    const user = userEvent.setup();
    await renderPage();
    await user.click(drawButton());
    await ask(user, 'draw a lighthouse');
    emit(toolStart);
    return screen.findByRole('list', { name: 'Tool execution status' });
  }

  it('says how far the tool has got, in words and a bar', async () => {
    const rows = await drawing();
    expect(within(rows).queryByRole('progressbar')).not.toBeInTheDocument();

    emit({ type: 'tool_progress', tool_call_id: 'call_1', stage: 'queued', position: 2 });
    expect(await within(rows).findByText('Queued, 2 in line')).toBeInTheDocument();
    expect(within(rows).getByRole('progressbar')).toHaveAttribute('aria-valuenow', '0');

    emit({ type: 'tool_progress', tool_call_id: 'call_1', stage: 'loading' });
    expect(await within(rows).findByText('Loading')).toBeInTheDocument();

    emit({ type: 'tool_progress', tool_call_id: 'call_1', stage: 'sampling', pass: 1, done: 5, total: 20 });
    expect(await within(rows).findByText('Sampling 5 of 20')).toBeInTheDocument();
    expect(within(rows).getByRole('progressbar')).toHaveAttribute('aria-valuenow', '25');

    emit({ type: 'tool_progress', tool_call_id: 'call_1', stage: 'decoding' });
    expect(await within(rows).findByText('Decoding')).toBeInTheDocument();
    expect(within(rows).getByRole('progressbar')).toHaveAttribute('aria-valuenow', '100');

    emit({ type: 'tool_progress', tool_call_id: 'call_1', stage: 'finishing' });
    expect(await within(rows).findByText('Finishing')).toBeInTheDocument();
  });

  it('shows the picture so far, the newest frame only, scaled up, and takes it away with the tool\'s result', async () => {
    const rows = await drawing();
    expect(within(rows).queryByRole('img')).not.toBeInTheDocument();

    preview('call_1', 3);
    const shown = await within(rows).findByRole('img', { name: 'Preview of the image being made, step 3 of 20' });
    expect(shown).toHaveAttribute('src', `data:image/png;base64,${PNG}3`);
    // About 128 px as it comes: drawn at twice that, smoothly.
    expect(shown).toHaveClass('w-[256px]', '[image-rendering:auto]');

    preview('call_1', 4);
    await within(rows).findByRole('img', { name: 'Preview of the image being made, step 4 of 20' });
    expect(within(rows).getAllByRole('img')).toHaveLength(1);

    emit(toolDone);
    await waitFor(() => expect(within(rows).queryByRole('img')).not.toBeInTheDocument());
    expect(within(rows).queryByRole('progressbar')).not.toBeInTheDocument();
    // A frame that comes after its tool's result is not drawn.
    preview('call_1', 20);
    emit({ type: 'text_delta', content: 'A lighthouse at dusk.' });
    await screen.findByText('A lighthouse at dusk.');
    expect(screen.queryByRole('img', { name: /Preview of the image/ })).not.toBeInTheDocument();
  });

  it('a reply queued behind a render says so, and the step that render is on', async () => {
    const user = userEvent.setup();
    await renderPage();
    await ask(user, 'hello');

    emit({ type: 'waiting', reason: 'image_render', step: 3, total: 20, position: 1 });
    const status = (await screen.findByText('Queued behind an image render')).closest('[role="status"]') as HTMLElement;
    const row = status.closest('.grid') as HTMLElement;
    expect(within(row).getByText('step 3 of 20')).toBeInTheDocument();

    emit({ type: 'waiting', reason: 'image_render', step: 9, total: 20, position: 1 });
    expect(await within(row).findByText('step 9 of 20')).toBeInTheDocument();
    expect(within(row).queryByText('step 3 of 20')).not.toBeInTheDocument();

    // The model reads the prompt: the wait is over.
    emit({ type: 'prompt_progress', processed: 10, total: 40, cached: 0, time_ms: 5 });
    await screen.findByText('Reading the prompt');
    expect(screen.queryByText('Queued behind an image render')).not.toBeInTheDocument();
    expect(screen.queryByText('step 9 of 20')).not.toBeInTheDocument();
  });

  it('a reply waiting for its model to load says that, with no step', async () => {
    const user = userEvent.setup();
    await renderPage();
    await ask(user, 'hello');

    emit({ type: 'waiting', reason: 'model_load', step: 0, total: 0, position: 0 });
    const status = (await screen.findByText('Waiting for the model to load')).closest('[role="status"]') as HTMLElement;
    expect(status.closest('.grid')).not.toHaveTextContent(/step \d+ of/);
  });
});
