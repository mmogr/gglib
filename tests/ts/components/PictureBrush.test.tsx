/**
 * The brush a part of a picture is marked with: strokes in the picture's own
 * pixels, whatever size it is shown at; one pointer at a time, the main
 * button or the first finger; and the brush row's size, Rub out, Undo and
 * Clear. What the strokes become (the mask sent) is the sheet's test.
 */

import { beforeAll, describe, it, expect } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';

import { BrushCanvas, BrushRow, useBrush } from '../../../src/components/ChatMessagesPanel/components/PictureBrush';
import type { MaskStroke } from '../../../src/utils/pictureMask';

const PICTURE = { width: 1000, height: 500 };

// jsdom has no canvas: the overlay is not painted, and the strokes are what is read.
beforeAll(() => {
  HTMLCanvasElement.prototype.getContext = (() => null) as never;
});

function Probe() {
  const brush = useBrush(PICTURE);
  return (
    <>
      <BrushRow picture={PICTURE} brush={brush} />
      <div>
        <BrushCanvas picture={PICTURE} brush={brush} />
      </div>
      <output aria-label="Strokes">{JSON.stringify(brush.strokes)}</output>
    </>
  );
}

const strokes = (): MaskStroke[] => JSON.parse(screen.getByLabelText('Strokes').textContent ?? '[]');

/** The canvas, shown 200 by 100: a fifth of the picture's size. */
function canvas(): HTMLElement {
  const shown = screen.getByLabelText('Brush over the part to fix');
  shown.getBoundingClientRect = () => ({ left: 0, top: 0, width: 200, height: 100 }) as DOMRect;
  return shown;
}

const at = (x: number, y: number, pointerId = 1) => ({ clientX: x, clientY: y, pointerId, isPrimary: true });

function brush(from: [number, number], to: [number, number]) {
  const shown = canvas();
  fireEvent.pointerDown(shown, at(...from));
  fireEvent.pointerMove(shown, at(...to));
  fireEvent.pointerUp(shown, at(...to));
}

describe('the picture brush', () => {
  it("brushes in the picture's own pixels, at 6% of its longer side to start", () => {
    render(<Probe />);
    brush([20, 10], [40, 30]);
    expect(strokes()).toEqual([{ points: [{ x: 100, y: 50 }, { x: 200, y: 150 }], width: 60 }]);
  });

  it('brushes at the size the brush row is set to', () => {
    render(<Probe />);
    const size = screen.getByLabelText('Brush size');
    expect(size).toHaveAttribute('min', '10');
    expect(size).toHaveAttribute('max', '250');
    fireEvent.change(size, { target: { value: '120' } });
    brush([20, 10], [40, 30]);
    expect(strokes()[0].width).toBe(120);
  });

  it('Undo takes the last stroke back and Clear takes them all; neither is offered with nothing brushed', () => {
    render(<Probe />);
    expect(screen.getByRole('button', { name: 'Undo' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Clear' })).toBeDisabled();
    brush([20, 10], [40, 30]);
    brush([100, 50], [120, 60]);
    brush([150, 50], [160, 60]);

    fireEvent.click(screen.getByRole('button', { name: 'Undo' }));
    expect(strokes().map((s) => s.points[0])).toEqual([{ x: 100, y: 50 }, { x: 500, y: 250 }]);

    fireEvent.click(screen.getByRole('button', { name: 'Clear' }));
    expect(strokes()).toEqual([]);
    expect(screen.getByRole('button', { name: 'Undo' })).toBeDisabled();
  });

  it('rubs out with a stroke that takes paint off, and Rub out again puts paint on', () => {
    render(<Probe />);
    brush([20, 10], [40, 30]);
    const rubOut = screen.getByRole('button', { name: 'Rub out' });
    fireEvent.click(rubOut);
    expect(rubOut).toHaveAttribute('aria-pressed', 'true');
    brush([30, 20], [40, 30]);
    fireEvent.click(rubOut);
    brush([50, 20], [60, 30]);
    expect(strokes().map((s) => s.erase ?? false)).toEqual([false, true, false]);
  });

  it('starts no stroke from a right click or a second finger', () => {
    render(<Probe />);
    const shown = canvas();
    fireEvent.pointerDown(shown, { ...at(5, 5, 2), button: 2 });
    fireEvent.pointerUp(shown, at(5, 5, 2));
    fireEvent.pointerDown(shown, { ...at(5, 5, 3), isPrimary: false });
    fireEvent.pointerMove(shown, { ...at(9, 9, 3), isPrimary: false });
    expect(strokes()).toEqual([]);
  });

  it('counts a press that reports the same place again as one point, which paints a dot', () => {
    render(<Probe />);
    const shown = canvas();
    fireEvent.pointerDown(shown, at(20, 10));
    fireEvent.pointerMove(shown, at(20, 10));
    fireEvent.pointerMove(shown, at(20, 10));
    fireEvent.pointerUp(shown, at(20, 10));
    expect(strokes()).toEqual([{ points: [{ x: 100, y: 50 }], width: 60 }]);
  });

  it('keeps a move back to where a stroke began, and one that changes only its height', () => {
    render(<Probe />);
    const shown = canvas();
    fireEvent.pointerDown(shown, at(20, 10));
    fireEvent.pointerMove(shown, at(20, 30));
    fireEvent.pointerMove(shown, at(20, 10));
    fireEvent.pointerUp(shown, at(20, 10));
    expect(strokes()[0].points).toEqual([{ x: 100, y: 50 }, { x: 100, y: 150 }, { x: 100, y: 50 }]);
  });

  it('follows one pointer through a stroke, and no other that moves or lifts meanwhile', () => {
    render(<Probe />);
    const shown = canvas();
    fireEvent.pointerDown(shown, at(10, 0));
    fireEvent.pointerMove(shown, at(20, 0, 2));
    fireEvent.pointerUp(shown, at(20, 0, 2));
    fireEvent.pointerMove(shown, at(20, 0));
    fireEvent.pointerMove(shown, at(90, 5));
    fireEvent.pointerUp(shown, at(90, 5));
    fireEvent.pointerMove(shown, at(30, 0));

    expect(strokes()).toHaveLength(1);
    expect(strokes()[0].points.map((point) => point.x)).toEqual([50, 100, 450]);
  });
});
