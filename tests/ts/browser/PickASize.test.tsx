/**
 * The size picker in a real engine: moving the selection and closing the
 * picker raise nothing.
 *
 * PickASize scrolls the selected row into view from an effect. React takes
 * an effect's return value as its cleanup, and calls it when the effect runs
 * again or the component unmounts. Chromium (156 here) and Edge (157, the
 * WebView2 of the desktop app on Windows) return a Promise from
 * scrollIntoView, so an effect that returns that call's value hands React a
 * cleanup that is not a function. WebKit returns undefined, and jsdom has no
 * scrollIntoView at all.
 */

import { afterEach, beforeEach, describe, expect, it, vi, type MockInstance } from 'vitest';
import { render } from '@testing-library/react';
import { PickASize } from '../../../src/inspector/hf/PickASize';
import type { HfQuantization } from '../../../src/types';

const MIB = 1024 * 1024;

const quant = (label: string, sizeMib: number): HfQuantization => ({
  label,
  role: 'Weights',
  file_path: `X-${label}.gguf`,
  size_bytes: sizeMib * MIB,
  size_mb: sizeMib,
  is_sharded: false,
  shard_count: null,
  projector: null,
  fit: null,
  fit_word: null,
  picked: false,
});

const QUANTS = [quant('Q4_K_M', 2400), quant('Q6_K', 3100), quant('Q8_0', 4000)];

const picker = (selected: HfQuantization) => (
  <PickASize quantizations={QUANTS} selected={selected} onSelect={() => {}} onDownload={() => {}} queueFull={false} />
);

describe('PickASize in a browser engine', () => {
  let consoleError: MockInstance<typeof console.error>;
  const uncaught: unknown[] = [];
  const onError = (event: ErrorEvent) => uncaught.push(event.error ?? event.message);

  beforeEach(() => {
    uncaught.length = 0;
    consoleError = vi.spyOn(console, 'error');
    window.addEventListener('error', onError);
  });

  afterEach(() => {
    window.removeEventListener('error', onError);
    vi.restoreAllMocks();
  });

  /** Select the first size, move to the last, close the picker. */
  const moveAndClose = () => {
    const view = render(picker(QUANTS[0]));
    expect(view.getByRole('option', { selected: true }).textContent).toContain('Q4_K_M');
    view.rerender(picker(QUANTS[2]));
    expect(view.getByRole('option', { selected: true }).textContent).toContain('Q8_0');
    view.unmount();
    // React reports an effect that returns anything but a function or
    // undefined with console.error in development.
    expect(consoleError).not.toHaveBeenCalled();
    expect(uncaught).toEqual([]);
  };

  it("raises nothing with this engine's own scrollIntoView", () => {
    // A spy that calls through, so the engine still answers: it holds the
    // test to a picker that did scroll.
    const scroll = vi.spyOn(Element.prototype, 'scrollIntoView');
    moveAndClose();
    expect(scroll).toHaveBeenCalledTimes(2);
  });

  // The same steps with scrollIntoView answering as Chromium and Edge do, so
  // an engine that returns undefined (WebKit) holds the picker to it too.
  it('raises nothing when scrollIntoView returns a Promise', () => {
    const scroll = vi
      .spyOn(Element.prototype, 'scrollIntoView')
      .mockImplementation(() => Promise.resolve() as unknown as undefined);
    moveAndClose();
    expect(scroll).toHaveBeenCalledTimes(2);
  });
});
