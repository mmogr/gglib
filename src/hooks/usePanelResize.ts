import { useCallback, useEffect, useRef, useState } from 'react';

interface UsePanelResizeConfig {
  initial: number;
  min: number;
  max: number;
}

export interface UsePanelResizeResult {
  leftPanelWidth: number;
  layoutRef: React.RefObject<HTMLDivElement | null>;
  handlePointerDown: (e: React.PointerEvent) => void;
  handleKeyboardResize: (delta: number) => void;
}

const KEYBOARD_STEP = 2;

/**
 * Manages drag-to-resize state for a two-panel layout.
 * Uses the Pointer Events API (pointermove/pointerup) throughout to avoid
 * the browser implicit pointer-capture issue that suppresses mousemove events
 * while a pointerdown target has capture.
 */
export function usePanelResize({ initial, min, max }: UsePanelResizeConfig): UsePanelResizeResult {
  const [leftPanelWidth, setLeftPanelWidth] = useState(initial);
  const layoutRef = useRef<HTMLDivElement>(null);
  const isDraggingRef = useRef(false);

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

export { KEYBOARD_STEP };
