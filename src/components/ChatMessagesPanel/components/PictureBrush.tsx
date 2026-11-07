/**
 * The brush a part of a picture is marked with, to be drawn again: the
 * strokes (`useBrush`), the canvas laid over the picture that takes them
 * (`BrushCanvas`), and the brush row (`BrushRow`): size, rub out, undo,
 * clear. Strokes are in the picture's own pixels, whatever size it is
 * shown at, so the mask made from them fits the picture.
 *
 * @module components/ChatMessagesPanel/components/PictureBrush
 */

import { FC, PointerEvent, useEffect, useRef, useState } from 'react';
import { Button } from '../../ui/Button';
import { paintOverlay, picturePoint, type MaskStroke } from '../../../utils/pictureMask';

/** The colour the brushed part is shown in, at the canvas's own part-clear. */
const TINT = '#ff4d4d';

/** A picture's size, in its own pixels. */
export interface PictureSize {
  width: number;
  height: number;
}

export interface Brush {
  strokes: readonly MaskStroke[];
  /** The brush's width, in the picture's pixels. */
  size: number;
  /** The brush takes paint off rather than putting it on. */
  erasing: boolean;
  setSize: (size: number) => void;
  setErasing: (erasing: boolean) => void;
  /** Start a stroke at a point of the picture. */
  begin: (at: MaskStroke['points'][number]) => void;
  /** Carry the stroke being made on to a point. */
  extend: (at: MaskStroke['points'][number]) => void;
  undo: () => void;
  clear: () => void;
}

/** The longer side of a picture, which the brush's sizes are shares of. */
const side = (picture: PictureSize) => Math.max(picture.width, picture.height);

/** The strokes brushed over `picture`, starting with none and a brush 6% of its longer side. */
export function useBrush(picture: PictureSize): Brush {
  const [strokes, setStrokes] = useState<MaskStroke[]>([]);
  const [size, setSize] = useState(() => Math.round(side(picture) * 0.06));
  const [erasing, setErasing] = useState(false);
  return {
    strokes,
    size,
    erasing,
    setSize,
    setErasing,
    begin: (at) => setStrokes((before) => [...before, { points: [at], width: size, ...(erasing && { erase: true }) }]),
    extend: (at) =>
      setStrokes((before) => {
        const last = before[before.length - 1];
        const end = last?.points[last.points.length - 1];
        // A move that has not moved adds nothing: a line of no length paints nothing.
        if (!last || (end && end.x === at.x && end.y === at.y)) return before;
        return [...before.slice(0, -1), { ...last, points: [...last.points, at] }];
      }),
    undo: () => setStrokes((before) => before.slice(0, -1)),
    clear: () => setStrokes([]),
  };
}

/** The canvas over the shown picture, its size, that a stroke is brushed on: the main button or the first finger alone. */
export const BrushCanvas: FC<{ picture: PictureSize; brush: Brush }> = ({ picture, brush }) => {
  const canvas = useRef<HTMLCanvasElement>(null);
  /** The pointer brushing now, by id; `null` for none. */
  const painting = useRef<number | null>(null);
  const { strokes } = brush;

  useEffect(() => {
    // jsdom has no 2D context: the strokes are still kept, only not shown.
    const ctx = canvas.current?.getContext('2d');
    if (ctx) paintOverlay(ctx, strokes, picture.width, picture.height, TINT);
  }, [strokes, picture.width, picture.height]);

  const pointOf = (event: PointerEvent<HTMLCanvasElement>) =>
    picturePoint({ x: event.clientX, y: event.clientY }, event.currentTarget.getBoundingClientRect(), picture);

  return (
    <canvas
      ref={canvas}
      width={picture.width}
      height={picture.height}
      aria-label="Brush over the part to fix"
      className="absolute inset-0 h-full w-full cursor-crosshair touch-none opacity-50"
      onPointerDown={(event) => {
        if (event.button > 0 || event.isPrimary === false) return;
        event.currentTarget.setPointerCapture?.(event.pointerId);
        painting.current = event.pointerId;
        brush.begin(pointOf(event));
      }}
      onPointerMove={(event) => {
        if (painting.current === event.pointerId) brush.extend(pointOf(event));
      }}
      onPointerUp={(event) => {
        if (painting.current === event.pointerId) painting.current = null;
      }}
      onPointerCancel={(event) => {
        if (painting.current === event.pointerId) painting.current = null;
      }}
    />
  );
};

/** The brush row: its size, from 1% to 25% of the picture's longer side; Rub out; Undo; Clear. */
export const BrushRow: FC<{ picture: PictureSize; brush: Brush; disabled?: boolean }> = ({ picture, brush, disabled }) => (
  <div className="flex flex-wrap items-center gap-sm" role="group" aria-label="Brush">
    <input
      type="range"
      className="min-w-[96px] flex-1 accent-primary"
      min={Math.round(side(picture) * 0.01)}
      max={Math.round(side(picture) * 0.25)}
      value={brush.size}
      disabled={disabled}
      onChange={(event) => brush.setSize(Number(event.target.value))}
      aria-label="Brush size"
    />
    <Button variant={brush.erasing ? 'secondary' : 'ghost'} size="sm" aria-pressed={brush.erasing} disabled={disabled} onClick={() => brush.setErasing(!brush.erasing)}>
      Rub out
    </Button>
    <Button variant="ghost" size="sm" onClick={brush.undo} disabled={disabled || brush.strokes.length === 0}>
      Undo
    </Button>
    <Button variant="ghost" size="sm" onClick={brush.clear} disabled={disabled || brush.strokes.length === 0}>
      Clear
    </Button>
  </div>
);
