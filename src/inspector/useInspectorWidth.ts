/**
 * The inspector's width: dragged by its left edge between 320 and 720 px,
 * and remembered per viewer. The Log subject opens at 60% of the window,
 * held to the same bounds; a drag there lasts while the log is open, and
 * leaving it returns to the remembered width.
 *
 * The edge also takes the arrow keys, 16 px a press. A viewer without
 * storage gets the 380 px default each time.
 *
 * @module inspector/useInspectorWidth
 */

import { useCallback, useRef, useState, type KeyboardEvent, type PointerEvent } from 'react';

export const MIN_WIDTH = 320;
export const MAX_WIDTH = 720;
const DEFAULT_WIDTH = 380;
const STEP = 16;
const KEY = 'gglib.inspector.width';

const clamp = (px: number) => Math.round(Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, px)));

function remembered(): number {
  try {
    const stored = Number(localStorage.getItem(KEY));
    return stored > 0 ? clamp(stored) : DEFAULT_WIDTH;
  } catch {
    return DEFAULT_WIDTH;
  }
}

function remember(px: number): void {
  try {
    localStorage.setItem(KEY, String(px));
  } catch {
    // Not remembered: the next visit starts at the default.
  }
}

export interface InspectorWidth {
  width: number;
  /** For the drag edge: `onPointerDown`, `onPointerMove`, `onPointerUp`, `onKeyDown`. */
  edge: {
    onPointerDown: (e: PointerEvent<HTMLElement>) => void;
    onPointerMove: (e: PointerEvent<HTMLElement>) => void;
    onPointerUp: (e: PointerEvent<HTMLElement>) => void;
    onKeyDown: (e: KeyboardEvent<HTMLElement>) => void;
  };
}

export function useInspectorWidth(log: boolean): InspectorWidth {
  const [saved, setSaved] = useState(remembered);
  // A drag while the log is open; each time it opens it starts at 60% of the window.
  const [logWidth, setLogWidth] = useState<number | null>(null);
  const [wasLog, setWasLog] = useState(log);
  if (wasLog !== log) {
    setWasLog(log);
    setLogWidth(null);
  }
  const dragging = useRef(false);

  const width = log ? (logWidth ?? clamp(window.innerWidth * 0.6)) : saved;

  const set = useCallback(
    (px: number) => {
      const next = clamp(px);
      if (log) setLogWidth(next);
      else {
        setSaved(next);
        remember(next);
      }
    },
    [log],
  );

  const edge: InspectorWidth['edge'] = {
    onPointerDown: (e) => {
      e.preventDefault();
      dragging.current = true;
      e.currentTarget.setPointerCapture?.(e.pointerId);
    },
    // The inspector is on the window's right: its width is what lies right of the pointer.
    onPointerMove: (e) => {
      if (dragging.current) set(window.innerWidth - e.clientX);
    },
    onPointerUp: (e) => {
      dragging.current = false;
      e.currentTarget.releasePointerCapture?.(e.pointerId);
    },
    onKeyDown: (e) => {
      if (e.key === 'ArrowLeft') set(width + STEP);
      else if (e.key === 'ArrowRight') set(width - STEP);
      else return;
      e.preventDefault();
    },
  };

  return { width, edge };
}
