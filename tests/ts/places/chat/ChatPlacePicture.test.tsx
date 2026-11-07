/**
 * The picture sheet, from a chat as the app routes to it: a picture in the
 * thread opens over the chat; the drawing tool's picture offers Fix a part
 * and Redo, another tool's or a question's own is only viewed. Fix sends
 * the brushed mask with the prompt, strength and image model; Redo sends
 * none. While it draws: the daemon's step and a bar, and Stop. After: Hold
 * to see the original (pointer and keyboard), Keep (saved after the picture
 * it came from, and shown in the thread), Try again, Discard. A failure is
 * said in the sheet. Esc with a drawn picture unkept asks first; closing
 * hands the focus back to the picture.
 *
 * jsdom has no canvas: `maskPng` is stubbed, and what it is given is the
 * mask. The brush's own geometry is `components/PictureBrush.test.tsx`'s.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { chatTransport, conversation, type ChatFixture } from './chatPlaceHarness';
import type { ChatMessage } from '../../../../src/services/transport';
import type { PictureRedraw, RedrawHandlers } from '../../../../src/services/transport/api/images';
import type { ImageModelRow } from '../../../../src/types/generated/ImageModelRow';

const transport = vi.hoisted(() => ({ current: {} as Record<string, unknown> }));
vi.mock('../../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../../src/services/transport');
  return { ...actual, getTransport: () => transport.current };
});
vi.mock('../../../../src/utils/pictureMask', async (original) => ({
  ...(await original<typeof import('../../../../src/utils/pictureMask')>()),
  maskPng: vi.fn(async () => new Blob(['mask'])),
}));

import { maskPng } from '../../../../src/utils/pictureMask';
import { confirm } from '../../../../src/app/confirm';
import { ingestServerEvent } from '../../../../src/services/serverRegistry';
import { renderApp } from '../harness';
import { progressLine, progressShare } from '../../../../src/components/ChatMessagesPanel/components/PictureWork';

const DRAWN = { id: 'd'.repeat(64), mime: 'image/png', width: 1000, height: 500 };
const SHOT = { id: 'e'.repeat(64), mime: 'image/png', width: 640, height: 480 };
const PASTED = { id: 'f'.repeat(64), mime: 'image/png', width: 800, height: 600 };
const KEPT = { id: 'a'.repeat(64), mime: 'image/png', width: 1000, height: 500 };

const rows = (): ChatMessage[] => [
  { id: 11, conversation_id: 1, role: 'user', content: 'Draw a fox, and look at this.', created_at: '2026-11-07T09:00:00Z', images: [PASTED] },
  {
    id: 12, conversation_id: 1, role: 'assistant', content: 'Here is your fox.', created_at: '2026-11-07T09:00:05Z',
    metadata: {
      tool_calls: [
        { id: 'draw_1', name: 'builtin:generate_image', arguments: { prompt: 'a red fox' } },
        { id: 'shot_1', name: 'builtin:screenshot', arguments: {} },
      ],
    },
  },
  { id: 13, conversation_id: 1, role: 'tool', content: '[image stored]', created_at: '2026-11-07T09:00:09Z', metadata: { tool_call_id: 'draw_1' }, images: [DRAWN] },
  { id: 14, conversation_id: 1, role: 'tool', content: '[image stored]', created_at: '2026-11-07T09:00:10Z', metadata: { tool_call_id: 'shot_1' }, images: [SHOT] },
];

const model = (id: number, name: string): ImageModelRow => ({ id, name, family: 'flux1', complete: true, missing: [], default: id === 1, resident: false });

let fixture: ChatFixture;
let handlers: RedrawHandlers;
const sent: PictureRedraw[] = [];
const stopRender = vi.fn();

beforeEach(() => {
  sent.length = 0;
  stopRender.mockClear();
  vi.mocked(maskPng).mockClear();
  HTMLCanvasElement.prototype.getContext = (() => null) as never;
  let made = 0;
  vi.spyOn(URL, 'createObjectURL').mockImplementation(() => `blob:shown-${++made}`);
  vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
  fixture = { conversations: [{ ...conversation(1, 'Foxes'), model_id: 41 }], rows: { 1: rows() }, runs: [], frames: {} };
  transport.current = {
    ...chatTransport(fixture),
    listModels: vi.fn(async () => []),
    fetchAttachmentBlob: vi.fn(async () => new Blob(['picture'], { type: 'image/png' })),
    drawingAvailability: vi.fn(async () => ({ available: true, models: [model(1, 'flux'), model(2, 'qwen-image')] })),
    redrawPicture: vi.fn((redraw: PictureRedraw, given: RedrawHandlers) => {
      sent.push(redraw);
      handlers = given;
      return stopRender;
    }),
    uploadAttachment: vi.fn(async () => ({ ...KEPT, image_tokens: 4 })),
    // The daemon saves it on the tool row of the picture it came from, after it.
    keepPicture: vi.fn(async (_cid: number, kept: { from: string; image: string }) => {
      const row = fixture.rows[1].find((r) => r.images?.some((i) => i.id === kept.from))!;
      row.images = [...row.images!, { ...KEPT, id: kept.image }];
      return row.images;
    }),
  };
  act(() => ingestServerEvent({ type: 'running', modelId: '41', port: 5041, updatedAt: Date.now(), modelName: 'qwen3-8b' }));
});
afterEach(() => {
  act(() => ingestServerEvent({ type: 'stopped', modelId: '41', port: 5041, updatedAt: Date.now() + 1 }));
  vi.restoreAllMocks();
});

const SLOW = { timeout: 5000 };
const sheet = () => screen.getByRole('dialog');
const strip = () => screen.getByRole('group', { name: 'Images from tools' });

/** The chat, its pictures read, and the one named `size` opened. */
async function open(size: string) {
  const user = userEvent.setup();
  const app = renderApp('/chat/1');
  const thumbnail = await screen.findByRole('button', { name: `Enlarge image, ${size}` }, SLOW);
  // The drawn picture is fixed only once the chat knows its image models.
  await waitFor(() => expect(transport.current.drawingAvailability).toHaveBeenCalled());
  await user.click(thumbnail);
  await screen.findByRole('dialog');
  return { user, app, thumbnail: () => screen.getByRole('button', { name: `Enlarge image, ${size}` }) };
}

/** Brush a stroke on the shown picture, shown 200 by 100. */
function brush() {
  const canvas = screen.getByLabelText('Brush over the part to fix');
  canvas.getBoundingClientRect = () => ({ left: 0, top: 0, width: 200, height: 100 }) as DOMRect;
  fireEvent.pointerDown(canvas, { clientX: 20, clientY: 10, pointerId: 1, isPrimary: true });
  fireEvent.pointerMove(canvas, { clientX: 40, clientY: 30, pointerId: 1 });
  fireEvent.pointerUp(canvas, { pointerId: 1 });
}

/** Redo with what is set, and the render drawn. */
async function redrawn(user: ReturnType<typeof userEvent.setup>) {
  await user.click(within(sheet()).getByRole('button', { name: 'Redo' }));
  await user.click(screen.getByRole('button', { name: 'Draw it again' }));
  await waitFor(() => expect(sent).toHaveLength(1));
  act(() => handlers.onDrawn(btoa('DRAWN')));
  await screen.findByAltText('The picture drawn again');
}

describe('the picture sheet', () => {
  it('opens the drawing tool\'s picture over the chat with Fix a part and Redo', async () => {
    await open('1000 × 500');
    expect(sheet()).toHaveAccessibleName('Picture');
    expect(within(sheet()).getByRole('img', { name: 'Image, 1000 × 500' })).toBeInTheDocument();
    expect(within(sheet()).getByRole('button', { name: 'Fix a part' })).toBeInTheDocument();
    expect(within(sheet()).getByRole('button', { name: 'Redo' })).toBeInTheDocument();
  });

  it('only shows another tool\'s picture, and a question\'s own', async () => {
    const { user } = await open('640 × 480');
    expect(within(sheet()).getByRole('img', { name: 'Image, 640 × 480' })).toBeInTheDocument();
    expect(within(sheet()).queryByRole('button', { name: 'Fix a part' })).not.toBeInTheDocument();
    expect(within(sheet()).queryByRole('button', { name: 'Redo' })).not.toBeInTheDocument();

    await user.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    await user.click(screen.getByRole('button', { name: 'Enlarge image, 800 × 600' }));
    expect(within(await screen.findByRole('dialog')).getByRole('img', { name: 'Image, 800 × 600' })).toBeInTheDocument();
    expect(within(sheet()).queryByRole('button', { name: 'Redo' })).not.toBeInTheDocument();
  });

  it('fixes a part: the brushed mask sent with the prompt, the strength and the image model', async () => {
    const { user } = await open('1000 × 500');
    await user.click(screen.getByRole('button', { name: 'Fix a part' }));
    expect(screen.getByRole('button', { name: 'Fix this part' })).toBeDisabled();
    expect(screen.getByText('Brush over the part to fix first.')).toBeInTheDocument();
    brush();
    fireEvent.change(screen.getByLabelText('How much to change'), { target: { value: '0.9' } });
    fireEvent.change(within(sheet()).getByLabelText('Image model'), { target: { value: '2' } });
    fireEvent.change(screen.getByRole('textbox'), { target: { value: ' a red fox, smiling ' } });

    await user.click(screen.getByRole('button', { name: 'Fix this part' }));

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toEqual({ prompt: 'a red fox, smiling', image: btoa('picture'), mask: btoa('mask'), strength: 0.9, model: 2 });
    expect(vi.mocked(maskPng).mock.calls[0]).toEqual([[{ points: [{ x: 100, y: 50 }, { x: 200, y: 150 }], width: 60 }], 1000, 500]);
  });

  it('redoes the whole picture: no mask, a redo\'s strength, the machine\'s own model unless one is chosen', async () => {
    const { user } = await open('1000 × 500');
    await user.click(screen.getByRole('button', { name: 'Redo' }));
    expect(screen.queryByLabelText('Brush over the part to fix')).not.toBeInTheDocument();
    expect(screen.getByRole('textbox')).toHaveValue('a red fox');

    await user.click(screen.getByRole('button', { name: 'Draw it again' }));

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toEqual({ prompt: 'a red fox', image: btoa('picture'), strength: 0.4 });
  });

  it('while it draws, says the daemon\'s step with a bar, and Stop stops it', async () => {
    const { user } = await open('1000 × 500');
    await user.click(screen.getByRole('button', { name: 'Redo' }));
    await user.click(screen.getByRole('button', { name: 'Draw it again' }));
    await waitFor(() => expect(sent).toHaveLength(1));

    act(() => handlers.onProgress({ stage: 'sampling', step: 6, total: 20, frame: 'RlJBTUU=' }));

    expect(within(sheet()).getByRole('status')).toHaveTextContent('step 6 of 20');
    const bar = within(sheet()).getByRole('progressbar');
    expect(bar).toHaveAttribute('aria-valuenow', '30');
    expect(bar.firstElementChild).toHaveStyle({ width: '30%' });
    expect(screen.getByAltText('The picture so far')).toHaveAttribute('src', 'data:image/png;base64,RlJBTUU=');

    await user.click(screen.getByRole('button', { name: 'Stop' }));
    expect(stopRender).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('button', { name: 'Draw it again' })).toBeEnabled();
  });

  it('holds the original up while pressed, by pointer and by keyboard', async () => {
    const { user } = await open('1000 × 500');
    const original = within(sheet()).getByRole('img', { name: 'Image, 1000 × 500' }).getAttribute('src');
    await redrawn(user);
    const hold = screen.getByRole('button', { name: 'Hold to see the original' });

    fireEvent.pointerDown(hold);
    expect(within(sheet()).getByRole('img', { name: 'Image, 1000 × 500' })).toHaveAttribute('src', original);
    fireEvent.pointerUp(hold);
    expect(screen.getByAltText('The picture drawn again')).toBeInTheDocument();
    fireEvent.pointerDown(hold);
    fireEvent.pointerCancel(hold);
    expect(screen.getByAltText('The picture drawn again')).toBeInTheDocument();
    fireEvent.pointerDown(hold);
    fireEvent.pointerLeave(hold);
    expect(screen.getByAltText('The picture drawn again')).toBeInTheDocument();

    fireEvent.keyDown(hold, { key: ' ' });
    expect(screen.queryByAltText('The picture drawn again')).not.toBeInTheDocument();
    fireEvent.keyUp(hold, { key: ' ' });
    expect(screen.getByAltText('The picture drawn again')).toBeInTheDocument();
    fireEvent.keyDown(hold, { key: 'Enter' });
    expect(screen.queryByAltText('The picture drawn again')).not.toBeInTheDocument();
    fireEvent.blur(hold);
    expect(screen.getByAltText('The picture drawn again')).toBeInTheDocument();
  });

  it('keeps the drawn picture on Keep, after the one it came from, and the thread shows it', async () => {
    const { user } = await open('1000 × 500');
    await redrawn(user);
    expect(transport.current.keepPicture).not.toHaveBeenCalled();

    await user.click(screen.getByRole('button', { name: 'Keep' }));

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(transport.current.keepPicture).toHaveBeenCalledWith(1, { from: DRAWN.id, image: KEPT.id });
    await waitFor(() => expect(within(strip()).getAllByRole('button', { name: 'Enlarge image, 1000 × 500' })).toHaveLength(2), SLOW);
    expect(transport.current.fetchAttachmentBlob).toHaveBeenCalledWith('this', KEPT.id);
  });

  it('says a Keep that failed in the sheet, keeps the drawn picture, and keeps it on a second Keep', async () => {
    const { user } = await open('1000 × 500');
    await redrawn(user);
    vi.mocked(transport.current.uploadAttachment as () => Promise<unknown>).mockRejectedValueOnce(new Error('the store is full'));

    await user.click(screen.getByRole('button', { name: 'Keep' }));

    expect(await within(sheet()).findByRole('alert')).toHaveTextContent('The picture was drawn but could not be kept: the store is full');
    expect(screen.getByAltText('The picture drawn again')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Keep' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(transport.current.keepPicture).toHaveBeenCalledTimes(1);
  });

  it('closes once the picture is kept, even when the chat cannot be read again just then', async () => {
    const { user } = await open('1000 × 500');
    await redrawn(user);
    const getThread = transport.current.getThread as ReturnType<typeof vi.fn>;
    getThread.mockRejectedValueOnce(new Error('the daemon is busy'));

    await user.click(screen.getByRole('button', { name: 'Keep' }));

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(transport.current.keepPicture).toHaveBeenCalledTimes(1);
  });

  it('tries again with the same values, and Discard keeps nothing', async () => {
    const { user } = await open('1000 × 500');
    await redrawn(user);

    await user.click(screen.getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(sent).toHaveLength(2));
    expect(sent[1]).toEqual(sent[0]);
    act(() => handlers.onDrawn(btoa('AGAIN')));

    await user.click(await screen.findByRole('button', { name: 'Discard' }));
    expect(screen.queryByAltText('The picture drawn again')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Draw it again' })).toBeEnabled();
    expect(transport.current.uploadAttachment).not.toHaveBeenCalled();
  });

  it('says a failure in the sheet, in the daemon\'s words, and may be asked again', async () => {
    const { user } = await open('1000 × 500');
    await user.click(screen.getByRole('button', { name: 'Redo' }));
    await user.click(screen.getByRole('button', { name: 'Draw it again' }));
    await waitFor(() => expect(sent).toHaveLength(1));

    act(() => handlers.onFailed('this model does not draw at 1000x500'));

    expect(within(sheet()).getByRole('alert')).toHaveTextContent('this model does not draw at 1000x500');
    expect(screen.getByRole('button', { name: 'Draw it again' })).toBeEnabled();
  });

  it('asks before Esc throws an unkept picture away, and hands the focus back to the picture', async () => {
    const { user, thumbnail } = await open('1000 × 500');
    await redrawn(user);

    await user.keyboard('{Escape}');
    const question = await screen.findByRole('dialog', { name: 'Discard the new picture?' });
    await user.click(within(question).getByRole('button', { name: 'Cancel' }));
    expect(screen.getByAltText('The picture drawn again')).toBeInTheDocument();

    await user.keyboard('{Escape}');
    await user.click(within(await screen.findByRole('dialog', { name: 'Discard the new picture?' })).getByRole('button', { name: 'Discard' }));

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(transport.current.uploadAttachment).not.toHaveBeenCalled();
    await waitFor(() => expect(thumbnail()).toHaveFocus());
  });

  it('closes on Esc at once with nothing drawn, and the focus goes back to the picture', async () => {
    const { user, thumbnail } = await open('1000 × 500');
    await user.click(screen.getByRole('button', { name: 'Fix a part' }));
    // Held in the sheet: Tab goes round its own controls and never out to the chat.
    for (let i = 0; i < 12; i += 1) {
      await user.tab();
      expect(sheet()).toContainElement(document.activeElement as HTMLElement);
    }

    await user.keyboard('{Escape}');

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    await waitFor(() => expect(thumbnail()).toHaveFocus());
  });

  it('closes while it draws, and the render nobody will read is stopped', async () => {
    const { user } = await open('1000 × 500');
    await user.click(screen.getByRole('button', { name: 'Redo' }));
    await user.click(screen.getByRole('button', { name: 'Draw it again' }));
    await waitFor(() => expect(sent).toHaveLength(1));

    await user.keyboard('{Escape}');

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(stopRender).toHaveBeenCalledTimes(1);
  });

  /** The dim backdrop around the sheet, which a click outside lands on. */
  const outside = () => document.querySelector('[data-modal-overlay]') as HTMLElement;

  it('Cancel in Fix a part and in Redo goes back to the picture, not out of the sheet', async () => {
    const { user } = await open('1000 × 500');
    await user.click(screen.getByRole('button', { name: 'Fix a part' }));
    expect(sheet()).toHaveAccessibleName('Fix a part');
    await user.click(within(sheet()).getByRole('button', { name: 'Cancel' }));
    expect(sheet()).toHaveAccessibleName('Picture');
    expect(screen.queryByLabelText('Brush over the part to fix')).not.toBeInTheDocument();

    await user.click(within(sheet()).getByRole('button', { name: 'Redo' }));
    expect(sheet()).toHaveAccessibleName('Redo');
    await user.click(within(sheet()).getByRole('button', { name: 'Cancel' }));
    expect(sheet()).toHaveAccessibleName('Picture');
    expect(within(sheet()).getByRole('button', { name: 'Fix a part' })).toBeInTheDocument();
  });

  it('× asks before it throws an unkept picture away', async () => {
    const { user } = await open('1000 × 500');
    await redrawn(user);

    await user.click(within(sheet()).getByRole('button', { name: 'Close' }));
    const question = await screen.findByRole('dialog', { name: 'Discard the new picture?' });
    await user.click(within(question).getByRole('button', { name: 'Cancel' }));
    expect(screen.getByAltText('The picture drawn again')).toBeInTheDocument();

    await user.click(within(screen.getByRole('dialog', { name: 'Keep this picture?' })).getByRole('button', { name: 'Close' }));
    await user.click(within(await screen.findByRole('dialog', { name: 'Discard the new picture?' })).getByRole('button', { name: 'Discard' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(transport.current.uploadAttachment).not.toHaveBeenCalled();
  });

  it('a click outside asks before it throws an unkept picture away: No keeps it, Discard closes', async () => {
    const { user } = await open('1000 × 500');
    await redrawn(user);

    await user.click(outside());
    await user.click(within(await screen.findByRole('dialog', { name: 'Discard the new picture?' })).getByRole('button', { name: 'Cancel' }));
    expect(screen.getByAltText('The picture drawn again')).toBeInTheDocument();

    await user.click(outside());
    await user.click(within(await screen.findByRole('dialog', { name: 'Discard the new picture?' })).getByRole('button', { name: 'Discard' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  });

  it('a click outside closes at once with nothing drawn', async () => {
    const { user } = await open('1000 × 500');
    await user.click(outside());
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  });

  it('a click outside while it draws closes, and stops the render once', async () => {
    const { user } = await open('1000 × 500');
    await user.click(screen.getByRole('button', { name: 'Redo' }));
    await user.click(screen.getByRole('button', { name: 'Draw it again' }));
    await waitFor(() => expect(sent).toHaveLength(1));

    await user.click(outside());

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(stopRender).toHaveBeenCalledTimes(1);
  });

  it('holds Keep, Try again and Discard while the picture is being kept', async () => {
    const { user } = await open('1000 × 500');
    await redrawn(user);
    (transport.current.uploadAttachment as ReturnType<typeof vi.fn>).mockImplementationOnce(() => new Promise(() => {}));

    await user.click(screen.getByRole('button', { name: 'Keep' }));

    expect(await screen.findByRole('button', { name: 'Keeping…' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Discard' })).toBeDisabled();
  });

  it('leaves another question open when the sheet goes after its own was answered', async () => {
    const { user, app } = await open('1000 × 500');
    await redrawn(user);
    await user.keyboard('{Escape}');
    await user.click(within(await screen.findByRole('dialog', { name: 'Discard the new picture?' })).getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Discard the new picture?' })).not.toBeInTheDocument());
    let answered: boolean | undefined;
    act(() => {
      void confirm({ title: 'Another question?', confirmLabel: 'Yes' }).then((yes) => {
        answered = yes;
      });
    });
    await screen.findByRole('dialog', { name: 'Another question?' });

    await act(async () => {
      await app.router.navigate('/library');
    });
    await act(async () => {
      await new Promise((r) => setTimeout(r, 50));
    });

    expect(screen.getByRole('dialog', { name: 'Another question?' })).toBeInTheDocument();
    expect(answered).toBeUndefined();
  });

  it('stays open on Esc while the picture is being kept', async () => {
    const { user } = await open('1000 × 500');
    await redrawn(user);
    (transport.current.uploadAttachment as ReturnType<typeof vi.fn>).mockImplementationOnce(() => new Promise(() => {}));
    await user.click(screen.getByRole('button', { name: 'Keep' }));
    await screen.findByRole('button', { name: 'Keeping…' });

    await user.keyboard('{Escape}');
    await act(async () => {
      await new Promise((r) => setTimeout(r, 50));
    });

    expect(screen.getByRole('dialog', { name: 'Keep this picture?' })).toBeInTheDocument();
    expect(screen.queryByRole('dialog', { name: 'Discard the new picture?' })).not.toBeInTheDocument();
  });

  it('answers its open question no when the sheet goes from under it', async () => {
    const { user, app } = await open('1000 × 500');
    await redrawn(user);
    await user.keyboard('{Escape}');
    await screen.findByRole('dialog', { name: 'Discard the new picture?' });

    await act(async () => {
      await app.router.navigate('/library');
    });

    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Discard the new picture?' })).not.toBeInTheDocument());
  });
});

describe('where a render is', () => {
  it('says each stage in words, the step from the daemon while it samples', () => {
    expect(progressLine(null)).toBe('Loading the image model…');
    expect(progressLine({ stage: 'loading' })).toBe('Loading the image model…');
    expect(progressLine({ stage: 'queued', position: 2 })).toBe('Waiting for the GPU, place 2 in line…');
    expect(progressLine({ stage: 'queued' })).toBe('Waiting for the GPU…');
    expect(progressLine({ stage: 'sampling', step: 6, total: 20 })).toBe('Drawing, step 6 of 20…');
    expect(progressLine({ stage: 'sampling' })).toBe('Drawing…');
    expect(progressLine({ stage: 'decoding' })).toBe('Finishing the picture…');
    expect(progressLine({ stage: 'finishing' })).toBe('Finishing the picture…');
  });

  it('fills the bar with the steps done, all of it once sampled, none before', () => {
    expect(progressShare(null)).toBe(0);
    expect(progressShare({ stage: 'queued', position: 1 })).toBe(0);
    expect(progressShare({ stage: 'sampling' })).toBe(0);
    expect(progressShare({ stage: 'sampling', step: 1, total: 3 })).toBe(33);
    // A step past the total, as sd-server's counts could say, fills the bar and no further.
    expect(progressShare({ stage: 'sampling', step: 25, total: 20 })).toBe(100);
    expect(progressShare({ stage: 'decoding' })).toBe(100);
    expect(progressShare({ stage: 'finishing' })).toBe(100);
  });
});
