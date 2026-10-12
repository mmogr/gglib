/**
 * The mask a person brushes: painted on black in the picture's own pixels,
 * white where a stroke paints and black again where one rubs out; where a
 * pointer is on the picture; and bytes to base64 and back.
 */

import { describe, it, expect } from 'vitest';
import {
  base64Of,
  marksSomething,
  paintMask,
  paintOverlay,
  picturePoint,
  pngOf,
  type MaskContext,
  type MaskStroke,
} from '../../../src/utils/pictureMask';
import { bytesOf } from '../fixtures/fakeImageStore';

/** A canvas context that writes down what it is asked, with the colour and width in force. */
function recorder(): { ctx: MaskContext; log: string[] } {
  const log: string[] = [];
  const ctx = {
    fillStyle: '',
    strokeStyle: '',
    lineWidth: 0,
    lineCap: 'butt',
    lineJoin: 'miter',
    globalCompositeOperation: 'source-over',
    fillRect: (x: number, y: number, w: number, h: number) => log.push(`fillRect ${x},${y} ${w}x${h} ${ctx.fillStyle}`),
    clearRect: (x: number, y: number, w: number, h: number) => log.push(`clearRect ${x},${y} ${w}x${h}`),
    beginPath: () => log.push('beginPath'),
    moveTo: (x: number, y: number) => log.push(`moveTo ${x},${y}`),
    lineTo: (x: number, y: number) => log.push(`lineTo ${x},${y}`),
    stroke: () => log.push(`stroke ${ctx.strokeStyle} ${ctx.lineWidth} ${ctx.lineCap} ${ctx.globalCompositeOperation}`),
    arc: (x: number, y: number, r: number) => log.push(`arc ${x},${y} r${r}`),
    fill: () => log.push(`fill ${ctx.fillStyle} ${ctx.globalCompositeOperation}`),
  };
  return { ctx: ctx as unknown as MaskContext, log };
}

const LINE: MaskStroke = { points: [{ x: 10, y: 20 }, { x: 30, y: 40 }], width: 8 };
const DOT: MaskStroke = { points: [{ x: 5, y: 6 }], width: 10 };
const RUB: MaskStroke = { points: [{ x: 10, y: 20 }, { x: 20, y: 30 }], width: 4, erase: true };

describe('paintMask', () => {
  it('fills the whole mask black before any stroke, so nothing is left clear', () => {
    const { ctx, log } = recorder();
    paintMask(ctx, [], 64, 48);
    expect(log).toEqual(['fillRect 0,0 64x48 #000000']);
  });

  it('paints a stroke white as a round line through its points, and a single point as a dot', () => {
    const { ctx, log } = recorder();
    paintMask(ctx, [LINE, DOT], 64, 48);
    expect(log).toEqual([
      'fillRect 0,0 64x48 #000000',
      'beginPath',
      'moveTo 10,20',
      'lineTo 30,40',
      'stroke #ffffff 8 round source-over',
      'beginPath',
      'arc 5,6 r5',
      'fill #ffffff source-over',
    ]);
  });

  it('paints a stroke that rubs out black, in its place in the order', () => {
    const { ctx, log } = recorder();
    paintMask(ctx, [LINE, RUB], 64, 48);
    expect(log.filter((line) => line.startsWith('stroke'))).toEqual([
      'stroke #ffffff 8 round source-over',
      'stroke #000000 4 round source-over',
    ]);
  });
});

describe('paintOverlay', () => {
  it('clears, tints what was painted, and takes the tint off where a stroke rubs out', () => {
    const { ctx, log } = recorder();
    paintOverlay(ctx, [LINE, RUB], 64, 48, '#ff0000');
    expect(log[0]).toBe('clearRect 0,0 64x48');
    expect(log.filter((line) => line.startsWith('stroke'))).toEqual([
      'stroke #ff0000 8 round source-over',
      'stroke #ff0000 4 round destination-out',
    ]);
    expect(ctx.globalCompositeOperation).toBe('source-over');
  });
});

describe('marksSomething', () => {
  it('is true once a stroke paints, and not for none or for rubbing out alone', () => {
    expect(marksSomething([])).toBe(false);
    expect(marksSomething([RUB])).toBe(false);
    expect(marksSomething([{ points: [], width: 4 }])).toBe(false);
    expect(marksSomething([RUB, DOT])).toBe(true);
  });
});

describe('picturePoint', () => {
  const box = { left: 100, top: 50, width: 200, height: 100 };
  const natural = { width: 1024, height: 512 };

  it('maps a pointer on the shown picture to the picture\'s own pixels', () => {
    expect(picturePoint({ x: 100, y: 50 }, box, natural)).toEqual({ x: 0, y: 0 });
    expect(picturePoint({ x: 200, y: 100 }, box, natural)).toEqual({ x: 512, y: 256 });
    expect(picturePoint({ x: 300, y: 150 }, box, natural)).toEqual({ x: 1024, y: 512 });
  });

  it('holds a pointer that has left the picture at its edge', () => {
    expect(picturePoint({ x: 0, y: 0 }, box, natural)).toEqual({ x: 0, y: 0 });
    expect(picturePoint({ x: 999, y: 999 }, box, natural)).toEqual({ x: 1024, y: 512 });
  });

  it('answers the corner for a picture not laid out yet', () => {
    expect(picturePoint({ x: 5, y: 5 }, { left: 0, top: 0, width: 0, height: 0 }, natural)).toEqual({ x: 0, y: 0 });
  });
});

describe('base64', () => {
  it('reads a blob as base64 and makes a PNG blob of it again, byte for byte', async () => {
    // Longer than one chunk, with every byte value.
    const bytes = Uint8Array.from({ length: 70_000 }, (_, at) => at % 256);
    const text = await base64Of(new Blob([bytes]));
    expect(text.startsWith('AAECAwQF')).toBe(true);

    const back = pngOf(text);
    expect(back.type).toBe('image/png');
    expect(await bytesOf(back)).toEqual(bytes);
  });
});
