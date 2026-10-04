/**
 * Refuses a file dragged over the page where nothing takes it, so the
 * desktop window, whose own drag and drop is off, never opens it in place
 * of the page and loses what was being written.
 *
 * Window listeners, in the bubble phase, act only on a drag that carries
 * files: a dragover no element has accepted is refused (`dropEffect`
 * `none`), and a drop is kept from the webview. A composer's dropzone takes
 * its drag first, in the capture phase, and keeps its own effect. Text and
 * links dragged within the page are left alone.
 *
 * @module useFileDropGuard
 */

import { useEffect } from 'react';

/** Whether a drag carries files from outside the page. */
const carriesFiles = (e: DragEvent): boolean => Array.from(e.dataTransfer?.types ?? []).includes('Files');

export function useFileDropGuard(): void {
  useEffect(() => {
    const onDragOver = (e: DragEvent) => {
      if (!carriesFiles(e) || e.defaultPrevented) return;
      e.preventDefault();
      if (e.dataTransfer) e.dataTransfer.dropEffect = 'none';
    };
    const onDrop = (e: DragEvent) => {
      if (carriesFiles(e)) e.preventDefault();
    };
    window.addEventListener('dragover', onDragOver);
    window.addEventListener('drop', onDrop);
    return () => {
      window.removeEventListener('dragover', onDragOver);
      window.removeEventListener('drop', onDrop);
    };
  }, []);
}
