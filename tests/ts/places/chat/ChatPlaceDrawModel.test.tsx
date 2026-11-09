/**
 * Choosing the image model on the chat page: `[+]` › Draw a picture ▸.
 *
 * It is a submenu only where the machine that draws lists two or more image
 * models, and not at all for a machine whose answer has no list. It lists
 * the machine's default rule, then each model in the machine's order with
 * its family and whether it is loaded; one that lacks a file cannot be
 * chosen and says what it lacks. A pick sends nothing: it sets Draw (the
 * `Draw ×` chip), and the run the next message starts says `draw` and
 * `image_model`. The chat keeps the pick: every later run that says `draw`
 * says it again, and a run without Draw says neither. A pick the machine
 * answers it cannot draw with unsets Draw, says that machine's reason and is
 * never sent; the submenu stays, and another pick, or the default, draws.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import type { LocalTurn } from '../../../../src/types/generated/LocalTurn';
import type { DrawingAvailability } from '../../../../src/types/generated/DrawingAvailability';
import type { ImageModelRow } from '../../../../src/types/generated/ImageModelRow';
import type { RunInfo } from '../../../../src/types/generated/RunInfo';
import { agentRun, chatTransport, conversation, plusItem, plusSubmenu, wrapper, type ChatFixture } from './chatPlaceHarness';

const transport = vi.hoisted(() => ({ current: {} as unknown }));
vi.mock('../../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../../src/services/transport');
  return { ...actual, getTransport: () => transport.current };
});

import ChatPlace from '../../../../src/places/chat/ChatPlace';

type User = ReturnType<typeof userEvent.setup>;
type Item = { type: 'end'; info: RunInfo };

const row = (id: number, name: string, more: Partial<ImageModelRow> = {}): ImageModelRow => ({
  id,
  name,
  family: 'flux1',
  complete: true,
  missing: [],
  default: false,
  resident: false,
  ...more,
});

// In the order a machine gives them: by family, then by name.
const QWEN = row(7, 'qwen-image-q8', { family: 'qwen-image-2.1' });
const FLUX = row(3, 'flux-schnell', { default: true, resident: true });
const SDXL = row(9, 'sdxl-base', { family: 'sdxl', complete: false, missing: ['vae', 'clip_l'] });
const ROWS = [QWEN, FLUX, SDXL];
const INCOMPLETE = 'sdxl-base has no file linked for vae, clip_l; link them with gglib model update';

let fixture: ChatFixture;
/** The machine's image models. `undefined` is a machine from before the list. */
let models: ImageModelRow[] | undefined;
/** The bodies of the runs started, in order. */
let started: LocalTurn[];
/** Ends the run going. */
let feed: (item: Item) => void;

/** What the machine answers for the image model asked about, as a daemon does. */
function answerFor(imageModel: number | null | undefined): DrawingAvailability {
  const listed = { models } as Pick<DrawingAvailability, 'models'>;
  if (imageModel == null) return { available: true, model: FLUX.name, model_id: FLUX.id, ...listed };
  const chosen = (models ?? []).find((each) => each.id === imageModel);
  if (!chosen) {
    const reason = `the image model chosen (id ${imageModel}) is no longer on this machine; choose another`;
    return { available: false, code: 'model_not_found', reason, ...listed };
  }
  if (!chosen.complete) return { available: false, code: 'image_model_incomplete', reason: INCOMPLETE, ...listed };
  return { available: true, model: chosen.name, model_id: chosen.id, ...listed };
}

function pageTransport() {
  return {
    ...chatTransport(fixture),
    drawingAvailability: vi.fn(async (_source: string, chat: { imageModel?: number | null } = {}) => answerFor(chat.imageModel)),
    startAgentRun: vi.fn(async (id: string, request: LocalTurn) => {
      started.push(request);
      return agentRun(id, 1, 'queued');
    }),
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
/** `[+]` › Draw a picture, the menu opened for it. */
const drawItem = () => plusItem('Draw a picture');
/** The `Draw ×` chip's ×, there only while the next message draws. */
const unDraw = () => screen.queryByRole('button', { name: "Don't draw" });
/** The image models' submenu, opened. */
const menu = (user: User) => plusSubmenu(user, 'Draw a picture');
/** Its choices, by name, and which one is ticked. */
async function choices(user: User): Promise<{ names: string[]; ticked: string | undefined }> {
  const items = within(await menu(user)).getAllByRole('menuitemradio');
  return {
    names: items.map((item) => item.textContent ?? ''),
    ticked: items.find((item) => item.getAttribute('aria-checked') === 'true')?.textContent ?? undefined,
  };
}
const noMenu = () => expect(drawItem()).not.toHaveAttribute('aria-haspopup');
/** Leave the menu, as a person does. */
const close = (user: User) => user.keyboard('{Escape}');
/** The image model each availability question asked about, in order. */
const askedAbout = () => stub().drawingAvailability.mock.calls.map(([, chat]) => chat?.imageModel ?? null);

async function renderPage() {
  render(<ChatPlace modelName="Qwen3.8-27B" modelId={7} serverPort={4321} conversationId={fixture.conversations[0]?.id ?? null} />, { wrapper });
  await waitFor(() => expect(screen.queryByText('Loading messages…')).not.toBeInTheDocument());
  await waitFor(() => expect(drawItem()).not.toHaveAttribute('title', 'Checking whether a message here can draw.'));
  await close(userEvent.setup());
}

/** Send `text`, wait for its run to be started, and end that run with a reply saved. */
async function ask(user: User, text: string) {
  const calls = stub().startAgentRun.mock.calls.length;
  await user.type(screen.getByRole('textbox', { name: 'Message' }), `${text}{Enter}`);
  await waitFor(() => expect(stub().startAgentRun).toHaveBeenCalledTimes(calls + 1));
  const reply = `reply ${calls + 1}`;
  fixture.rows[1] = [
    ...(fixture.rows[1] ?? []),
    { id: 90 + calls, conversation_id: 1, role: 'assistant', content: reply, created_at: '2026-10-10T10:00:00Z' },
  ];
  feed({ type: 'end', info: agentRun('r', 1, 'completed') });
  await screen.findByText(reply);
  await waitFor(() => expect(screen.queryByRole('button', { name: 'Stop' })).not.toBeInTheDocument());
}

/** Pick `name` in the submenu, and wait for the machine's answer about it. */
async function pick(user: User, name: string | RegExp) {
  await user.click(within(await menu(user)).getByRole('menuitemradio', { name }));
  await waitFor(() => expect(drawItem()).not.toHaveAttribute('title', 'Checking whether a message here can draw.'));
  await close(user);
}

beforeEach(() => {
  window.localStorage.clear();
  fixture = { conversations: [conversation(1, 'Lighthouses')], rows: {}, runs: [], frames: {} };
  models = ROWS;
  started = [];
  feed = () => {};
  transport.current = pageTransport();
});

describe('ChatPlace, Draw a picture ▸', () => {
  it('is not a submenu with no image model, with one, or for a machine whose answer has no list', async () => {
    for (const listed of [[], [FLUX], undefined]) {
      models = listed;
      const { unmount } = render(<ChatPlace modelName="Qwen3.8-27B" modelId={7} serverPort={4321} conversationId={fixture.conversations[0]?.id ?? null} />, { wrapper });
      await waitFor(() => expect(drawItem()).toBeEnabled());
      noMenu();
      expect(drawItem()).toHaveAttribute(
        'title',
        'Let the next message draw a picture with flux-schnell. It applies to that message only.',
      );
      unmount();
    }
  });

  it('lists the default rule, then each model as the machine orders them, with its family and whether it is loaded', async () => {
    const user = userEvent.setup();
    await renderPage();

    expect(drawItem()).toHaveAttribute('aria-haspopup', 'menu');
    expect(await choices(user)).toEqual({
      names: ['Default (flux-schnell)', 'qwen-image-q8 · Qwen-Image 2.1', 'flux-schnell · Flux.1 · loaded', 'sdxl-base · SDXL · incomplete'],
      ticked: 'Default (flux-schnell)',
    });
    expect(unDraw()).not.toBeInTheDocument();
  });

  it('a model that lacks a file cannot be chosen, and says what it lacks', async () => {
    const user = userEvent.setup();
    await renderPage();

    const [, qwen, flux, sdxl] = within(await menu(user)).getAllByRole('menuitemradio');
    expect(sdxl).toBeDisabled();
    expect(sdxl).toHaveAttribute('title', 'Missing: VAE, CLIP-L');
    expect(qwen).toBeEnabled();
    expect(qwen).not.toHaveAttribute('title');
    expect(flux).toBeEnabled();
  });

  it('a pick sends nothing and sets Draw; the run that message starts says draw and the model', async () => {
    const user = userEvent.setup();
    await renderPage();

    await pick(user, /^qwen-image-q8/);
    expect(started).toEqual([]);
    expect((await choices(user)).ticked).toMatch(/^qwen-image-q8/);
    await close(user);
    expect(unDraw()).toBeInTheDocument();
    expect(askedAbout().at(-1)).toBe(7);

    await ask(user, 'draw a lighthouse');
    expect(started[0].draw).toBe(true);
    expect(started[0].image_model).toBe(7);
  });

  it('the chat keeps the pick: a run without Draw says neither key, and the next one with Draw says the model again', async () => {
    const user = userEvent.setup();
    await renderPage();
    await pick(user, /^qwen-image-q8/);
    await ask(user, 'draw a lighthouse');
    await waitFor(() => expect(unDraw()).not.toBeInTheDocument());

    await ask(user, 'thanks');
    expect(Object.keys(started[1])).not.toContain('draw');
    expect(Object.keys(started[1])).not.toContain('image_model');
    expect(drawItem()).toHaveAttribute(
      'title',
      'Let the next message draw a picture with qwen-image-q8. It applies to that message only.',
    );
    expect((await choices(user)).ticked).toMatch(/^qwen-image-q8/);

    await pick(user, /^qwen-image-q8/);
    await ask(user, 'and one at dawn');
    expect(started[2].draw).toBe(true);
    expect(started[2].image_model).toBe(7);
  });

  it('picking the default again forgets the pick: the next run that draws names no model', async () => {
    const user = userEvent.setup();
    await renderPage();
    await pick(user, /^qwen-image-q8/);
    await pick(user, /^Default/);
    expect((await choices(user)).ticked).toMatch(/^Default/);
    await close(user);
    expect(askedAbout().at(-1)).toBeNull();
    expect(unDraw()).toBeInTheDocument();

    await ask(user, 'draw a lighthouse');
    expect(started[0].draw).toBe(true);
    expect(Object.keys(started[0])).not.toContain('image_model');
  });

  it('a pick the machine cannot draw with unsets Draw, says its reason and is never sent; the submenu stays, and another pick draws', async () => {
    const user = userEvent.setup();
    await renderPage();
    // The model leaves the machine after the list was read.
    models = [FLUX, SDXL];
    await pick(user, /^qwen-image-q8/);

    // Asked about, refused, and shown as chosen still.
    expect(askedAbout().at(-1)).toBe(7);
    const REASON = 'The image model chosen (id 7) is no longer on this machine; choose another';
    await waitFor(() => expect(drawItem()).toHaveAttribute('title', REASON));
    expect(drawItem()).toBeEnabled();
    await close(user);
    expect(unDraw()).not.toBeInTheDocument();
    const open = await menu(user);
    expect(within(open).getByText(REASON)).toBeInTheDocument();
    expect(within(open).getByRole('menuitemradio', { name: 'Model 7 (no longer listed)' })).toHaveAttribute('aria-checked', 'true');
    await close(user);

    await ask(user, 'draw it anyway');
    const refused = started.at(-1)!;
    expect(Object.keys(refused)).not.toContain('draw');
    expect(Object.keys(refused)).not.toContain('image_model');

    await pick(user, /^flux-schnell/);
    expect(drawItem()).toBeEnabled();
    expect(unDraw()).toBeInTheDocument();
    await ask(user, 'draw a lighthouse');
    expect(started.at(-1)!.draw).toBe(true);
    expect(started.at(-1)!.image_model).toBe(3);
  });

  it('stays while a pick stands though the list has shrunk to one, so the pick can be taken back', async () => {
    const user = userEvent.setup();
    await renderPage();
    // Every model but one leaves the machine after the list was read.
    models = [FLUX];
    await pick(user, /^qwen-image-q8/);

    await waitFor(() => expect(drawItem()).toHaveAttribute('title', 'The image model chosen (id 7) is no longer on this machine; choose another'));
    await close(user);
    expect(await choices(user)).toEqual({
      names: ['Default (flux-schnell)', 'Model 7 (no longer listed)', 'flux-schnell · Flux.1 · loaded'],
      ticked: 'Model 7 (no longer listed)',
    });
    await close(user);

    await pick(user, /^Default/);
    expect(drawItem()).toBeEnabled();
    expect(askedAbout().at(-1)).toBeNull();
    // Nothing chosen and one model: there is nothing to choose again.
    noMenu();
  });
});
