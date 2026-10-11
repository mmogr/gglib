/**
 * A run's preview frames, as the page holds them: a `preview` event's data
 * read as a frame or not at all, one frame a tool call, and a call's frame
 * dropped without touching another's.
 */

import { describe, it, expect } from 'vitest';
import {
  NO_PREVIEWS,
  parsePreview,
  previewSrc,
  withPreview,
  withoutPreview,
} from '../../../../src/hooks/useGglibRuntime/runPreviews';

const frame = (step: number) => ({ mime: 'image/png', step, total: 20, b64: `iVBORw0KGgo${step}` });
const data = (toolCallId: unknown, f: unknown) => JSON.stringify({ tool_call_id: toolCallId, frame: f });

describe('parsePreview', () => {
  it('reads the data of a preview event (contracts/runs/preview_stream.txt)', () => {
    const wire = '{"tool_call_id":"call_1","frame":{"mime":"image/png","step":3,"total":20,"b64":"iVBORw0KGgo="}}';
    expect(parsePreview(wire)).toEqual({
      tool_call_id: 'call_1',
      frame: { mime: 'image/png', step: 3, total: 20, b64: 'iVBORw0KGgo=' },
    });
  });

  it('reads nothing from data that is not a frame of an image', () => {
    expect(parsePreview('nope')).toBeNull();
    expect(parsePreview('null')).toBeNull();
    expect(parsePreview(data('call_1', undefined))).toBeNull();
    expect(parsePreview(data(7, frame(1)))).toBeNull();
    expect(parsePreview(data('call_1', { ...frame(1), b64: '' }))).toBeNull();
    expect(parsePreview(data('call_1', { ...frame(1), b64: 7 }))).toBeNull();
    expect(parsePreview(data('call_1', { ...frame(1), mime: 'text/html' }))).toBeNull();
    expect(parsePreview(data('call_1', { ...frame(1), mime: 'image/png;base64,AAAA" onerror="x' }))).toBeNull();
    expect(parsePreview(data('call_1', { ...frame(1), mime: undefined }))).toBeNull();
  });
});

describe('the frames held', () => {
  it('keeps one frame a call, the newest, and never changes the map it was given', () => {
    const one = withPreview(NO_PREVIEWS, { tool_call_id: 'a', frame: frame(1) });
    const two = withPreview(one, { tool_call_id: 'a', frame: frame(2) });
    const both = withPreview(two, { tool_call_id: 'b', frame: frame(5) });

    expect(NO_PREVIEWS.size).toBe(0);
    expect([...one]).toEqual([['a', frame(1)]]);
    expect([...two]).toEqual([['a', frame(2)]]);
    expect([...both]).toEqual([['a', frame(2)], ['b', frame(5)]]);
  });

  it('drops one call\'s frame and keeps the others; a call with none changes nothing', () => {
    const both = withPreview(withPreview(NO_PREVIEWS, { tool_call_id: 'a', frame: frame(2) }), { tool_call_id: 'b', frame: frame(5) });

    expect([...withoutPreview(both, 'a')]).toEqual([['b', frame(5)]]);
    expect(both.size).toBe(2);
    expect(withoutPreview(both, 'c')).toBe(both);
  });

  it('is shown as a data URL of its own type', () => {
    expect(previewSrc(frame(3))).toBe('data:image/png;base64,iVBORw0KGgo3');
  });
});
