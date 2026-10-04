/**
 * A file dragged where nothing takes it is refused, so the desktop window
 * never opens it in place of the page; a drag an element accepted, and one
 * that carries no files, are left as they are.
 *
 * jsdom has no `DataTransfer`, so each event's is the plain object given,
 * read back after the event.
 */

import { describe, it, expect, afterEach } from 'vitest';
import { fireEvent, renderHook } from '@testing-library/react';
import { useFileDropGuard } from '../../../src/hooks/useFileDropGuard';

/** A drag's `dataTransfer`, carrying `types`, its effect as the page leaves it. */
const drag = (...types: string[]) => ({ types, dropEffect: 'copy' });

describe('useFileDropGuard', () => {
  let zone: HTMLDivElement | null = null;
  afterEach(() => {
    zone?.remove();
    zone = null;
  });

  it('refuses a file dragged over the page and keeps its drop from the window', () => {
    renderHook(() => useFileDropGuard());
    const over = drag('Files');

    expect(fireEvent.dragOver(document.body, { dataTransfer: over })).toBe(false);
    expect(over.dropEffect).toBe('none');
    expect(fireEvent.drop(document.body, { dataTransfer: drag('Files') })).toBe(false);
  });

  it('leaves a drag that carries no files alone', () => {
    renderHook(() => useFileDropGuard());
    expect(fireEvent.dragOver(document.body, { dataTransfer: drag('Files') })).toBe(false);

    const over = drag('text/plain');
    expect(fireEvent.dragOver(document.body, { dataTransfer: over })).toBe(true);
    expect(over.dropEffect).toBe('copy');
    expect(fireEvent.drop(document.body, { dataTransfer: drag('text/plain', 'text/uri-list') })).toBe(true);
  });

  it('keeps the effect of a drag an element has accepted', () => {
    renderHook(() => useFileDropGuard());
    zone = document.body.appendChild(document.createElement('div'));
    zone.addEventListener('dragover', (e) => {
      e.preventDefault();
      e.dataTransfer!.dropEffect = 'copy';
    });
    const over = drag('Files');

    expect(fireEvent.dragOver(zone, { dataTransfer: over })).toBe(false);
    expect(over.dropEffect).toBe('copy');
  });

  it('refuses nothing once it is gone', () => {
    const hook = renderHook(() => useFileDropGuard());
    expect(fireEvent.dragOver(document.body, { dataTransfer: drag('Files') })).toBe(false);

    hook.unmount();
    const over = drag('Files');
    expect(fireEvent.dragOver(document.body, { dataTransfer: over })).toBe(true);
    expect(over.dropEffect).toBe('copy');
    expect(fireEvent.drop(document.body, { dataTransfer: drag('Files') })).toBe(true);
  });
});
