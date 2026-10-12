/**
 * One picture drawn again, from the request to the picture kept in the
 * chat: what the machine is asked, the render followed, stopped or failed,
 * and the result stored and linked only when Keep is pressed.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';

import type { PictureRedraw, RedrawHandlers } from '../../../src/services/transport/api/images';

const transport = vi.hoisted(() => ({
  redrawPicture: vi.fn(),
  uploadAttachment: vi.fn(),
  keepPicture: vi.fn(),
}));
vi.mock('../../../src/services/transport', () => ({ getTransport: () => transport }));
// jsdom paints no canvas: the mask's bytes are the strokes it was asked for.
vi.mock('../../../src/utils/pictureMask', async (original) => ({
  ...(await original<typeof import('../../../src/utils/pictureMask')>()),
  maskPng: vi.fn(async (strokes: unknown, width: number, height: number) =>
    new Blob([`mask ${JSON.stringify(strokes)} ${width}x${height}`]),
  ),
}));

import { usePictureRedraw, type RedrawAsk, type RedrawSource } from '../../../src/components/ChatMessagesPanel/hooks/usePictureRedraw';

const SOURCE: RedrawSource = { id: 'a'.repeat(64), width: 64, height: 48, blob: async () => new Blob(['picture']) };
const STROKE = { points: [{ x: 1, y: 2 }], width: 4 };
const ASK: RedrawAsk = { prompt: 'a hand', strength: 0.75, strokes: [STROKE], model: null };
/** "DRAWN", base64. */
const DRAWN = 'RFJBV04=';

let handlers: RedrawHandlers;
let sent: PictureRedraw;
const stopRender = vi.fn();
const onKept = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  transport.redrawPicture.mockImplementation((redraw: PictureRedraw, given: RedrawHandlers) => {
    sent = redraw;
    handlers = given;
    return stopRender;
  });
  transport.uploadAttachment.mockResolvedValue({ id: 'b'.repeat(64), mime: 'image/png', width: 64, height: 48, image_tokens: 4 });
  transport.keepPicture.mockResolvedValue([]);
});

async function started(ask = ASK) {
  const hook = renderHook(() => usePictureRedraw(SOURCE, 7, onKept));
  act(() => hook.result.current.start(ask));
  await waitFor(() => expect(transport.redrawPicture).toHaveBeenCalled());
  return hook;
}

describe('usePictureRedraw', () => {
  it('sends the picture, the mask of what was brushed at the picture\'s size, and the strength', async () => {
    await started();

    expect(sent).toEqual({
      prompt: 'a hand',
      image: btoa('picture'),
      mask: btoa(`mask ${JSON.stringify([STROKE])} 64x48`),
      strength: 0.75,
    });
  });

  it('sends no mask for a whole redraw, and the model when one is chosen', async () => {
    await started({ ...ASK, strokes: [], model: 5 });

    expect(sent).toEqual({ prompt: 'a hand', image: btoa('picture'), strength: 0.75, model: 5 });
  });

  it('follows the render, keeping the last frame through a report that has none', async () => {
    const { result } = await started();
    expect(result.current.state).toEqual({ phase: 'drawing', progress: null });

    act(() => handlers.onProgress({ stage: 'sampling', step: 1, total: 4, frame: 'F1' }));
    act(() => handlers.onProgress({ stage: 'decoding' }));

    expect(result.current.state).toEqual({ phase: 'drawing', progress: { stage: 'decoding', frame: 'F1' } });
  });

  it('holds the drawn picture and stores nothing until Keep is pressed', async () => {
    const { result } = await started();
    act(() => handlers.onDrawn(DRAWN));

    expect(result.current.state.phase).toBe('drawn');
    expect(transport.uploadAttachment).not.toHaveBeenCalled();

    await act(() => result.current.keep());

    const [store, picture] = transport.uploadAttachment.mock.calls[0] as [string, Blob];
    expect(store).toBe('this');
    expect(picture.type).toBe('image/png');
    expect(picture.size).toBe(5);
    expect(transport.keepPicture).toHaveBeenCalledWith(7, { from: SOURCE.id, image: 'b'.repeat(64) });
    expect(onKept).toHaveBeenCalledTimes(1);
  });

  it('throws a drawn picture away on Discard, keeping nothing', async () => {
    const { result } = await started();
    act(() => handlers.onDrawn(DRAWN));

    act(() => result.current.discard());

    expect(result.current.state).toEqual({ phase: 'idle' });
    await act(() => result.current.keep());
    expect(transport.uploadAttachment).not.toHaveBeenCalled();
  });

  it('stops the render on Stop and drops what it says afterwards', async () => {
    const { result } = await started();

    act(() => result.current.stop());
    act(() => handlers.onDrawn(DRAWN));

    expect(stopRender).toHaveBeenCalledTimes(1);
    expect(result.current.state).toEqual({ phase: 'idle' });
  });

  it('stops a render still going when the page lets go of it', async () => {
    const { unmount } = await started();
    unmount();
    expect(stopRender).toHaveBeenCalledTimes(1);
  });

  it('says why a render failed', async () => {
    const { result } = await started();
    act(() => handlers.onFailed('the mask has no white in it'));
    expect(result.current.state).toEqual({ phase: 'failed', message: 'the mask has no white in it' });
  });

  it('keeps hold of a drawn picture a Keep could not keep, so it can be kept again', async () => {
    const { result } = await started();
    act(() => handlers.onDrawn(DRAWN));
    transport.keepPicture.mockRejectedValueOnce(new Error('no such chat'));

    await act(() => result.current.keep());

    expect(result.current.state).toMatchObject({
      phase: 'drawn',
      problem: 'The picture was drawn but could not be kept: no such chat',
    });
    expect(onKept).not.toHaveBeenCalled();

    await act(() => result.current.keep());

    expect(transport.keepPicture).toHaveBeenCalledTimes(2);
    expect(onKept).toHaveBeenCalledTimes(1);
  });

  it('does not call a kept picture unkept when the chat cannot be shown again', async () => {
    const { result } = await started();
    act(() => handlers.onDrawn(DRAWN));
    onKept.mockRejectedValueOnce(new Error('the chat could not be read'));

    await act(() => result.current.keep());

    expect(transport.keepPicture).toHaveBeenCalledTimes(1);
    expect(result.current.state.phase).toBe('keeping');
  });

  it('sends nothing for a redraw stopped, or let go of, while its picture was still being read', async () => {
    let read: (blob: Blob) => void = () => {};
    const slow: RedrawSource = { ...SOURCE, blob: () => new Promise<Blob>((resolve) => (read = resolve)) };
    const stopped = renderHook(() => usePictureRedraw(slow, 7, onKept));
    act(() => stopped.result.current.start(ASK));
    act(() => stopped.result.current.stop());
    await act(async () => read(new Blob(['picture'])));

    const gone = renderHook(() => usePictureRedraw(slow, 7, onKept));
    act(() => gone.result.current.start(ASK));
    gone.unmount();
    await act(async () => read(new Blob(['picture'])));
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(transport.redrawPicture).not.toHaveBeenCalled();
  });

  it('fails when the picture itself cannot be read', async () => {
    const unread: RedrawSource = { ...SOURCE, blob: async () => Promise.reject(new Error('gone from the store')) };
    const { result } = renderHook(() => usePictureRedraw(unread, 7, onKept));

    act(() => result.current.start(ASK));

    await waitFor(() => expect(result.current.state).toEqual({ phase: 'failed', message: 'gone from the store' }));
    expect(transport.redrawPicture).not.toHaveBeenCalled();
  });
});
