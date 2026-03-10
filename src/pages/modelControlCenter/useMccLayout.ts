import { useCallback, useEffect, useRef, useState } from 'react';

export interface UseMccLayoutResult {
  leftPanelWidth: number;
  layoutRef: React.RefObject<HTMLDivElement | null>;
  handlePointerDown: (e: React.PointerEvent) => void;
  handleKeyboardResize: (delta: number) => void;
}

const MIN_WIDTH = 25;
const MAX_WIDTH = 60;

export function useMccLayout(): UseMccLayoutResult {
  const [leftPanelWidth, setLeftPanelWidth] = useState(45);
  const layoutRef = useRef<HTMLDivElement>(null);
  const isDraggingRef = useRef(false);

  const handlePointerDown = useCallback((e: React.PointerEvent) => {
    e.preventDefault();
    isDraggingRef.current = true;
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  }, []);

  const handleKeyboardResize = useCallback((delta: number) => {
    setLeftPanelWidth(prev => Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, prev + delta)));
  }, []);

  useEffect(() => {
    let rafId: number | null = null;

    const handleMouseMove = (e: MouseEvent) => {
      if (!isDraggingRef.current || !layoutRef.current) return;
      if (rafId !== null) cancelAnimationFrame(rafId);

      rafId = requestAnimationFrame(() => {
        if (!layoutRef.current) return;
        const rect = layoutRef.current.getBoundingClientRect();
        const x = e.clientX - rect.left;
        const percentage = (x / rect.width) * 100;
        const newLeftWidth = Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, percentage));
        setLeftPanelWidth(newLeftWidth);
      });
    };

    const handleMouseUp = () => {
      if (rafId !== null) cancelAnimationFrame(rafId);
      isDraggingRef.current = false;
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);

    return () => {
      if (rafId !== null) cancelAnimationFrame(rafId);
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, []);

  return { leftPanelWidth, layoutRef, handlePointerDown, handleKeyboardResize };
}
