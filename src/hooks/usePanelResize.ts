import { useCallback, useEffect, useRef, useState } from 'react';

interface UsePanelResizeConfig {
  initial: number;
  min: number;
  max: number;
  /**
   * localStorage key for persisting the split. Omit to keep the width
   * in-memory only (resets on reload).
   */
  storageKey?: string;
}

/**
 * Read a persisted split width, clamped to the configured bounds.
 * Falls back to `initial` for missing, malformed, or out-of-range values,
 * and when storage is unavailable (private mode, disabled cookies).
 */
function readStoredWidth(storageKey: string | undefined, initial: number, min: number, max: number): number {
  if (!storageKey) return initial;
  try {
    const raw = window.localStorage.getItem(storageKey);
    if (raw === null) return initial;
    const parsed = Number.parseFloat(raw);
    if (!Number.isFinite(parsed)) return initial;
    return Math.max(min, Math.min(max, parsed));
  } catch {
    return initial;
  }
}

export interface UsePanelResizeResult {
  leftPanelWidth: number;
  layoutRef: React.RefObject<HTMLDivElement | null>;
  handlePointerDown: (e: React.PointerEvent) => void;
  handleKeyboardResize: (delta: number) => void;
}

/**
 * Manages drag-to-resize state for a two-panel layout.
 * Uses the Pointer Events API (pointermove/pointerup) throughout to avoid
 * the browser implicit pointer-capture issue that suppresses mousemove events
 * while a pointerdown target has capture.
 */
export function usePanelResize({ initial, min, max, storageKey }: UsePanelResizeConfig): UsePanelResizeResult {
  const [leftPanelWidth, setLeftPanelWidth] = useState(() =>
    readStoredWidth(storageKey, initial, min, max),
  );
  const layoutRef = useRef<HTMLDivElement>(null);
  const isDraggingRef = useRef(false);

  // Persist the split so it survives reloads and app restarts.
  useEffect(() => {
    if (!storageKey) return;
    try {
      window.localStorage.setItem(storageKey, String(leftPanelWidth));
    } catch {
      // Storage unavailable — the split simply stays in-memory.
    }
  }, [storageKey, leftPanelWidth]);

  const handlePointerDown = useCallback((e: React.PointerEvent) => {
    e.preventDefault();
    // Release implicit pointer capture so pointermove events bubble to document.
    (e.currentTarget as Element).releasePointerCapture(e.pointerId);
    isDraggingRef.current = true;
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  }, []);

  const handleKeyboardResize = useCallback((delta: number) => {
    setLeftPanelWidth(prev => Math.max(min, Math.min(max, prev + delta)));
  }, [min, max]);

  useEffect(() => {
    let rafId: number | null = null;

    const handlePointerMove = (e: PointerEvent) => {
      if (!isDraggingRef.current || !layoutRef.current) return;
      if (rafId !== null) cancelAnimationFrame(rafId);

      rafId = requestAnimationFrame(() => {
        if (!layoutRef.current) return;
        const rect = layoutRef.current.getBoundingClientRect();
        const x = e.clientX - rect.left;
        const percentage = (x / rect.width) * 100;
        setLeftPanelWidth(Math.max(min, Math.min(max, percentage)));
      });
    };

    const handlePointerUp = () => {
      if (rafId !== null) cancelAnimationFrame(rafId);
      isDraggingRef.current = false;
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };

    document.addEventListener('pointermove', handlePointerMove);
    document.addEventListener('pointerup', handlePointerUp);

    return () => {
      if (rafId !== null) cancelAnimationFrame(rafId);
      document.removeEventListener('pointermove', handlePointerMove);
      document.removeEventListener('pointerup', handlePointerUp);
    };
  }, [min, max]);

  return { leftPanelWidth, layoutRef, handlePointerDown, handleKeyboardResize };
}
