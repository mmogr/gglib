/**
 * Fixing or redoing a drawn picture from the thread: where the button is
 * offered, what each choice asks the machine for, and that nothing joins
 * the chat until Keep.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom';

import type { PictureRedraw, RedrawHandlers } from '../../../src/services/transport/api/images';

const transport = vi.hoisted(() => ({
  redrawPicture: vi.fn(),
  uploadAttachment: vi.fn(),
  keepPicture: vi.fn(),
  fetchAttachmentBlob: vi.fn(),
}));
vi.mock('../../../src/services/transport', () => ({ getTransport: () => transport }));
vi.mock('../../../src/utils/pictureMask', async (original) => ({
  ...(await original<typeof import('../../../src/utils/pictureMask')>()),
  maskPng: vi.fn(async () => new Blob(['mask'])),
}));

import { MessageActionsContext, type PictureActions } from '../../../src/components/ChatMessagesPanel/components/MessageActionsContext';
import { ToolImages } from '../../../src/components/ChatMessagesPanel/components/MessageImages';
import { PictureEditor, progressLine } from '../../../src/components/ChatMessagesPanel/components/PictureEditor';
import { replyImages } from '../../../src/types/messages';
import type { ImageModelRow } from '../../../src/types/generated/ImageModelRow';

const PICTURE = { id: 'a'.repeat(64), mime: 'image/png', width: 1000, height: 500 };
const model = (id: number, name: string, complete = true): ImageModelRow => ({
  id,
  name,
  family: 'flux1',
  complete,
  missing: [],
  default: id === 1,
  resident: false,
});

let handlers: RedrawHandlers;
let sent: PictureRedraw;
const onKept = vi.fn();
const onClose = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  URL.createObjectURL = vi.fn(() => 'blob:shown');
  URL.revokeObjectURL = vi.fn();
  transport.fetchAttachmentBlob.mockResolvedValue(new Blob(['picture']));
  transport.redrawPicture.mockImplementation((redraw: PictureRedraw, given: RedrawHandlers) => {
    sent = redraw;
    handlers = given;
    return vi.fn();
  });
  transport.uploadAttachment.mockResolvedValue({ id: 'b'.repeat(64), mime: 'image/png', width: 1000, height: 500, image_tokens: 4 });
  transport.keepPicture.mockResolvedValue([]);
});

function editor(models: ImageModelRow[] = [], picked: number | null = null) {
  render(
    <PictureEditor
      open
      onClose={onClose}
      picture={PICTURE}
      url="blob:original"
      prompt="a woman holding a cup"
      conversationId={7}
      models={models}
      picked={picked}
      onKept={onKept}
    />,
  );
}

/** Brush from one place on the shown picture to another; the picture is shown 200 by 100. */
function brush(from: [number, number], to: [number, number]) {
  const canvas = screen.getByLabelText('Brush over the part to fix');
  canvas.getBoundingClientRect = () => ({ left: 0, top: 0, width: 200, height: 100 }) as DOMRect;
  fireEvent.pointerDown(canvas, { clientX: from[0], clientY: from[1], pointerId: 1, isPrimary: true });
  fireEvent.pointerMove(canvas, { clientX: to[0], clientY: to[1], pointerId: 1 });
  fireEvent.pointerUp(canvas, { pointerId: 1 });
}

async function sentRedraw(): Promise<PictureRedraw> {
  await waitFor(() => expect(transport.redrawPicture).toHaveBeenCalled());
  return sent;
}

describe('PictureEditor', () => {
  it('will not fix until something is brushed, and says so', () => {
    editor();
    expect(screen.getByRole('button', { name: 'Fix this part' })).toBeDisabled();
    expect(screen.getByText('Brush over the part to fix first.')).toBeVisible();

    brush([20, 10], [40, 30]);

    expect(screen.getByRole('button', { name: 'Fix this part' })).toBeEnabled();
    expect(screen.queryByText('Brush over the part to fix first.')).not.toBeInTheDocument();
  });

  it('fixes the brushed part: the draw\'s own prompt, a mask, and a fix\'s strength', async () => {
    editor();
    brush([20, 10], [40, 30]);

    fireEvent.click(screen.getByRole('button', { name: 'Fix this part' }));

    expect(await sentRedraw()).toEqual({
      prompt: 'a woman holding a cup',
      image: btoa('picture'),
      mask: btoa('mask'),
      strength: 0.75,
    });
  });

  it('brushes in the picture\'s own pixels, and Undo takes the last stroke back', async () => {
    const { maskPng } = await import('../../../src/utils/pictureMask');
    editor();
    brush([20, 10], [40, 30]);
    brush([100, 50], [120, 60]);
    fireEvent.click(screen.getByRole('button', { name: 'Undo' }));

    fireEvent.click(screen.getByRole('button', { name: 'Fix this part' }));
    await sentRedraw();

    // Shown at a fifth of its size: 20,10 on screen is 100,50 of the picture.
    expect(vi.mocked(maskPng).mock.calls[0]).toEqual([
      [{ points: [{ x: 100, y: 50 }, { x: 200, y: 150 }], width: 60 }],
      1000,
      500,
    ]);
  });

  it('redoes the whole picture with no mask, at a redo\'s strength and the words as edited', async () => {
    editor();
    brush([20, 10], [40, 30]);
    fireEvent.click(screen.getByRole('button', { name: 'Redo the whole picture' }));
    fireEvent.change(screen.getByRole('textbox'), { target: { value: '  a woman holding a mug ' } });

    fireEvent.click(screen.getByRole('button', { name: 'Draw it again' }));

    expect(await sentRedraw()).toEqual({ prompt: 'a woman holding a mug', image: btoa('picture'), strength: 0.4 });
  });

  it('will not draw with no words', () => {
    editor();
    fireEvent.click(screen.getByRole('button', { name: 'Redo the whole picture' }));
    fireEvent.change(screen.getByRole('textbox'), { target: { value: '   ' } });

    expect(screen.getByRole('button', { name: 'Draw it again' })).toBeDisabled();
    expect(screen.getByText('Say what the picture should show.')).toBeVisible();
  });

  it('rubs out with a stroke that takes paint off, and brushes with the main button alone', async () => {
    const { maskPng } = await import('../../../src/utils/pictureMask');
    editor();
    brush([20, 10], [40, 30]);
    fireEvent.click(screen.getByRole('button', { name: 'Rub out' }));
    brush([30, 20], [40, 30]);
    const canvas = screen.getByLabelText('Brush over the part to fix');
    // Neither a right click nor a second finger starts a stroke.
    fireEvent.pointerDown(canvas, { clientX: 5, clientY: 5, pointerId: 2, button: 2, isPrimary: true });
    fireEvent.pointerUp(canvas, { pointerId: 2 });
    fireEvent.pointerDown(canvas, { clientX: 5, clientY: 5, pointerId: 3, isPrimary: false });
    fireEvent.pointerUp(canvas, { pointerId: 3 });

    fireEvent.click(screen.getByRole('button', { name: 'Fix this part' }));
    await sentRedraw();

    const strokes = vi.mocked(maskPng).mock.calls[0][0];
    expect(strokes.map((stroke) => stroke.erase ?? false)).toEqual([false, true]);
  });

  it('counts a press that reports the same place again as one point, which paints a dot', async () => {
    const { maskPng } = await import('../../../src/utils/pictureMask');
    editor();
    const canvas = screen.getByLabelText('Brush over the part to fix');
    canvas.getBoundingClientRect = () => ({ left: 0, top: 0, width: 200, height: 100 }) as DOMRect;
    const here = { clientX: 20, clientY: 10, pointerId: 1, isPrimary: true };
    fireEvent.pointerDown(canvas, here);
    fireEvent.pointerMove(canvas, here);
    fireEvent.pointerMove(canvas, here);
    fireEvent.pointerUp(canvas, here);

    fireEvent.click(screen.getByRole('button', { name: 'Fix this part' }));
    await sentRedraw();

    expect(vi.mocked(maskPng).mock.calls[0][0]).toEqual([{ points: [{ x: 100, y: 50 }], width: 60 }]);
  });

  it('keeps a move back to where a stroke began, and one that changes only its height', async () => {
    const { maskPng } = await import('../../../src/utils/pictureMask');
    editor();
    const canvas = screen.getByLabelText('Brush over the part to fix');
    canvas.getBoundingClientRect = () => ({ left: 0, top: 0, width: 200, height: 100 }) as DOMRect;
    const at = (x: number, y: number) => ({ clientX: x, clientY: y, pointerId: 1, isPrimary: true });
    fireEvent.pointerDown(canvas, at(20, 10));
    fireEvent.pointerMove(canvas, at(20, 30));
    fireEvent.pointerMove(canvas, at(20, 10));
    fireEvent.pointerUp(canvas, at(20, 10));

    fireEvent.click(screen.getByRole('button', { name: 'Fix this part' }));
    await sentRedraw();

    const points = vi.mocked(maskPng).mock.calls[0][0][0].points;
    expect(points).toEqual([{ x: 100, y: 50 }, { x: 100, y: 150 }, { x: 100, y: 50 }]);
  });

  it('follows one pointer through a stroke, and no other that moves or lifts meanwhile', async () => {
    const { maskPng } = await import('../../../src/utils/pictureMask');
    editor();
    const canvas = screen.getByLabelText('Brush over the part to fix');
    canvas.getBoundingClientRect = () => ({ left: 0, top: 0, width: 200, height: 100 }) as DOMRect;
    const at = (x: number, pointerId: number) => ({ clientX: x, clientY: 10, pointerId, isPrimary: true });
    fireEvent.pointerDown(canvas, at(10, 0));
    fireEvent.pointerMove(canvas, at(20, 0));
    fireEvent.pointerMove(canvas, at(90, 5));
    fireEvent.pointerUp(canvas, at(90, 5));
    fireEvent.pointerMove(canvas, at(30, 0));
    fireEvent.pointerUp(canvas, at(30, 0));
    fireEvent.pointerMove(canvas, at(40, 0));

    fireEvent.click(screen.getByRole('button', { name: 'Fix this part' }));
    await sentRedraw();

    const strokes = vi.mocked(maskPng).mock.calls[0][0];
    expect(strokes).toHaveLength(1);
    expect(strokes[0].points.map((point) => point.x)).toEqual([50, 100, 150]);
  });

  it('sends the strength the slider is moved to', async () => {
    editor();
    fireEvent.click(screen.getByRole('button', { name: 'Redo the whole picture' }));
    fireEvent.change(screen.getByLabelText('How much to change'), { target: { value: '0.9' } });

    fireEvent.click(screen.getByRole('button', { name: 'Draw it again' }));

    expect((await sentRedraw()).strength).toBe(0.9);
  });

  it('offers a choice of image model only where more than one can draw, and sends the one chosen', async () => {
    editor([model(1, 'flux'), model(2, 'qwen-image'), model(3, 'half', false)]);
    const menu = screen.getByLabelText('Image model');
    expect(menu.querySelectorAll('option')).toHaveLength(3);

    fireEvent.change(menu, { target: { value: '2' } });
    fireEvent.click(screen.getByRole('button', { name: 'Redo the whole picture' }));
    fireEvent.click(screen.getByRole('button', { name: 'Draw it again' }));

    expect((await sentRedraw()).model).toBe(2);
  });

  it('starts the choice at the image model the chat has chosen', async () => {
    editor([model(1, 'flux'), model(2, 'qwen-image')], 2);
    expect(screen.getByLabelText('Image model')).toHaveValue('2');

    fireEvent.click(screen.getByRole('button', { name: 'Redo the whole picture' }));
    fireEvent.click(screen.getByRole('button', { name: 'Draw it again' }));

    expect((await sentRedraw()).model).toBe(2);
  });

  it('falls back to the machine\'s own choice for a chosen model it can no longer draw with', async () => {
    editor([model(1, 'flux'), model(2, 'qwen-image'), model(3, 'half', false)], 3);
    expect(screen.getByLabelText('Image model')).toHaveValue('default');

    fireEvent.click(screen.getByRole('button', { name: 'Redo the whole picture' }));
    fireEvent.click(screen.getByRole('button', { name: 'Draw it again' }));

    expect(await sentRedraw()).not.toHaveProperty('model');
  });

  it('shows no model menu for one image model', () => {
    editor([model(1, 'flux'), model(3, 'half', false)]);
    expect(screen.queryByLabelText('Image model')).not.toBeInTheDocument();
  });

  it('says where the render is while it draws, with Stop in place of the choices', async () => {
    editor();
    fireEvent.click(screen.getByRole('button', { name: 'Redo the whole picture' }));
    fireEvent.click(screen.getByRole('button', { name: 'Draw it again' }));
    await sentRedraw();

    act(() => handlers.onProgress({ stage: 'sampling', step: 3, total: 20, frame: 'RlJBTUU=' }));

    expect(screen.getByRole('status')).toHaveTextContent('Drawing, step 3 of 20…');
    expect(screen.getByAltText('The picture so far')).toHaveAttribute('src', 'data:image/png;base64,RlJBTUU=');
    expect(screen.queryByRole('button', { name: 'Draw it again' })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Stop' }));
    expect(screen.getByRole('button', { name: 'Draw it again' })).toBeEnabled();
  });

  it('keeps the drawn picture in the chat only on Keep, then tells the chat and closes', async () => {
    editor();
    fireEvent.click(screen.getByRole('button', { name: 'Redo the whole picture' }));
    fireEvent.click(screen.getByRole('button', { name: 'Draw it again' }));
    await sentRedraw();
    act(() => handlers.onDrawn(btoa('DRAWN')));

    expect(screen.getByAltText('The picture drawn again')).toBeInTheDocument();
    expect(transport.uploadAttachment).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: 'Keep' }));

    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(transport.keepPicture).toHaveBeenCalledWith(7, { from: PICTURE.id, image: 'b'.repeat(64) });
    expect(onKept).toHaveBeenCalledTimes(1);
  });

  it('keeps a drawn picture through a Keep that fails, and through Escape', async () => {
    editor();
    fireEvent.click(screen.getByRole('button', { name: 'Redo the whole picture' }));
    fireEvent.click(screen.getByRole('button', { name: 'Draw it again' }));
    await sentRedraw();
    act(() => handlers.onDrawn(btoa('DRAWN')));
    transport.uploadAttachment.mockRejectedValueOnce(new Error('the store is full'));

    fireEvent.click(screen.getByRole('button', { name: 'Keep' }));

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'The picture was drawn but could not be kept: the store is full',
    );
    expect(screen.getByAltText('The picture drawn again')).toBeInTheDocument();

    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByAltText('The picture drawn again')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Keep' }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(transport.keepPicture).toHaveBeenCalledTimes(1);
  });

  it('closes once the picture is kept, even when the chat cannot be shown again', async () => {
    onKept.mockRejectedValueOnce(new Error('the chat could not be read'));
    editor();
    fireEvent.click(screen.getByRole('button', { name: 'Redo the whole picture' }));
    fireEvent.click(screen.getByRole('button', { name: 'Draw it again' }));
    await sentRedraw();
    act(() => handlers.onDrawn(btoa('DRAWN')));

    fireEvent.click(screen.getByRole('button', { name: 'Keep' }));

    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(transport.keepPicture).toHaveBeenCalledTimes(1);
  });

  it('shows the original while the compare button is held', async () => {
    editor();
    fireEvent.click(screen.getByRole('button', { name: 'Redo the whole picture' }));
    fireEvent.click(screen.getByRole('button', { name: 'Draw it again' }));
    await sentRedraw();
    act(() => handlers.onDrawn(btoa('DRAWN')));
    const hold = screen.getByRole('button', { name: 'Hold to see the original' });

    fireEvent.pointerDown(hold);
    expect(screen.getByAltText('The picture')).toHaveAttribute('src', 'blob:original');

    fireEvent.pointerUp(hold);
    expect(screen.getByAltText('The picture drawn again')).toHaveAttribute('src', 'blob:shown');

    // Held by the keyboard, then left: it lets go.
    fireEvent.keyDown(hold, { key: ' ' });
    expect(screen.getByAltText('The picture')).toBeInTheDocument();
    fireEvent.blur(hold);
    expect(screen.getByAltText('The picture drawn again')).toBeInTheDocument();
  });

  it('asks again with the same choices on Try again, and keeps nothing on Discard', async () => {
    editor();
    brush([20, 10], [40, 30]);
    fireEvent.click(screen.getByRole('button', { name: 'Fix this part' }));
    const first = await sentRedraw();
    act(() => handlers.onDrawn(btoa('DRAWN')));

    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(transport.redrawPicture).toHaveBeenCalledTimes(2));
    expect(sent).toEqual(first);

    act(() => handlers.onDrawn(btoa('AGAIN')));
    fireEvent.click(screen.getByRole('button', { name: 'Discard' }));

    expect(screen.getByRole('button', { name: 'Fix this part' })).toBeEnabled();
    expect(transport.uploadAttachment).not.toHaveBeenCalled();
  });

  it('says why a render failed and lets it be asked again', async () => {
    editor();
    fireEvent.click(screen.getByRole('button', { name: 'Redo the whole picture' }));
    fireEvent.click(screen.getByRole('button', { name: 'Draw it again' }));
    await sentRedraw();

    act(() => handlers.onFailed('this model does not draw at 1000x500'));

    expect(screen.getByRole('alert')).toHaveTextContent('this model does not draw at 1000x500');
    expect(screen.getByRole('button', { name: 'Draw it again' })).toBeEnabled();
  });
});

describe('progressLine', () => {
  it('says each stage in words', () => {
    expect(progressLine(null)).toBe('Loading the image model…');
    expect(progressLine({ stage: 'loading' })).toBe('Loading the image model…');
    expect(progressLine({ stage: 'queued', position: 2 })).toBe('Waiting for the GPU, place 2 in line…');
    expect(progressLine({ stage: 'queued' })).toBe('Waiting for the GPU…');
    expect(progressLine({ stage: 'sampling' })).toBe('Drawing…');
    expect(progressLine({ stage: 'decoding' })).toBe('Finishing the picture…');
    expect(progressLine({ stage: 'finishing' })).toBe('Finishing the picture…');
  });
});

describe('the button on a drawn picture', () => {
  const pictures: PictureActions = { conversationId: 7, models: [], picked: null, onKept };

  async function enlarged(prompt: string | undefined, actions: PictureActions | undefined, source: 'this' | 'far' = 'this') {
    render(
      <MessageActionsContext.Provider value={{ onDeleteMessage: vi.fn(), source, ...(actions && { pictures: actions }) }}>
        <ToolImages images={[{ image: PICTURE, ...(prompt && { prompt }) }]} />
      </MessageActionsContext.Provider>,
    );
    fireEvent.click(await screen.findByRole('button', { name: /Enlarge image/ }));
  }

  it('is offered on a picture the drawing tool made, and opens the editor with its prompt', async () => {
    await enlarged('a red fox', pictures);

    fireEvent.click(screen.getByRole('button', { name: 'Fix or redo…' }));

    expect(await screen.findByRole('textbox')).toHaveValue('a red fox');
    expect(screen.getByRole('button', { name: 'Fix this part' })).toBeInTheDocument();
  });

  it('is not offered on another tool\'s image, or where the chat cannot keep one', async () => {
    await enlarged(undefined, pictures);
    expect(screen.queryByRole('button', { name: 'Fix or redo…' })).not.toBeInTheDocument();
  });

  it('is not offered where the chat has no picture actions, as a far chat has none', async () => {
    await enlarged('a red fox', undefined, 'far');
    expect(screen.queryByRole('button', { name: 'Fix or redo…' })).not.toBeInTheDocument();
  });
});

describe('replyImages', () => {
  const image = { id: 'c'.repeat(64), mime: 'image/png', width: 64, height: 64 };
  const call = (toolName: string, args: unknown) => ({ type: 'tool-call', toolName, args, artifact: { images: [image] } });
  const saved = { custom: { dbId: 12 } };

  it('gives a saved reply\'s drawn picture its prompt, and no other tool\'s image one', () => {
    const content = [
      { type: 'text', text: 'Here.' },
      call('builtin:generate_image', { prompt: 'a red fox' }),
      call('builtin:read_file', { prompt: 'a red fox' }),
      call('builtin:generate_image', { prompt: '  ' }),
      call('builtin:generate_image', null),
      { type: 'tool-call', toolName: 'builtin:generate_image', args: { prompt: 'none drawn' } },
    ];
    expect(replyImages({ content, metadata: saved })).toEqual([{ image, prompt: 'a red fox' }, { image }, { image }, { image }]);
  });

  it('gives none a prompt on a reply that is not a saved row: one arriving, or drawn and not read back', () => {
    const content = [call('builtin:generate_image', { prompt: 'a red fox' })];
    expect(replyImages({ content })).toEqual([{ image }]);
    expect(replyImages({ content, metadata: { custom: {} } })).toEqual([{ image }]);
    expect(replyImages({ content: 'plain words', metadata: saved })).toEqual([]);
  });
});
