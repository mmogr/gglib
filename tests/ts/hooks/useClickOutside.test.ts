/**
 * Tests for useClickOutside hook.
 * 
 * Tests click outside detection and event handling.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useRef } from 'react';
import { useClickOutside } from '../../../src/hooks/useClickOutside';

describe('useClickOutside', () => {
  let container: HTMLDivElement;
  let target: HTMLDivElement;
  let outside: HTMLDivElement;

  beforeEach(() => {
    // Set up DOM structure
    container = document.createElement('div');
    target = document.createElement('div');
    target.id = 'target';
    outside = document.createElement('div');
    outside.id = 'outside';
    
    container.appendChild(target);
    container.appendChild(outside);
    document.body.appendChild(container);
  });

  afterEach(() => {
    document.body.removeChild(container);
  });

  it('calls handler when clicking outside the referenced element', () => {
    const handler = vi.fn();

    renderHook(() => {
      const ref = useRef<HTMLDivElement>(target);
      useClickOutside(ref, handler);
      return ref;
    });

    // Simulate click outside
    const event = new MouseEvent('mousedown', { bubbles: true });
    outside.dispatchEvent(event);

    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('does not call handler when clicking inside the referenced element', () => {
    const handler = vi.fn();

    renderHook(() => {
      const ref = useRef<HTMLDivElement>(target);
      useClickOutside(ref, handler);
      return ref;
    });

    // Simulate click inside
    const event = new MouseEvent('mousedown', { bubbles: true });
    target.dispatchEvent(event);

    expect(handler).not.toHaveBeenCalled();
  });

  it('does not call handler when clicking on a child of the referenced element', () => {
    const handler = vi.fn();
    const child = document.createElement('span');
    target.appendChild(child);

    renderHook(() => {
      const ref = useRef<HTMLDivElement>(target);
      useClickOutside(ref, handler);
      return ref;
    });

    // Simulate click on child
    const event = new MouseEvent('mousedown', { bubbles: true });
    child.dispatchEvent(event);

    expect(handler).not.toHaveBeenCalled();
  });

  it('respects enabled flag - does not listen when disabled', () => {
    const handler = vi.fn();

    renderHook(() => {
      const ref = useRef<HTMLDivElement>(target);
      useClickOutside(ref, handler, false); // disabled
      return ref;
    });

    // Simulate click outside
    const event = new MouseEvent('mousedown', { bubbles: true });
    outside.dispatchEvent(event);

    expect(handler).not.toHaveBeenCalled();
  });

  it('starts listening when enabled becomes true', () => {
    const handler = vi.fn();

    const { rerender } = renderHook(
      ({ enabled }) => {
        const ref = useRef<HTMLDivElement>(target);
        useClickOutside(ref, handler, enabled);
        return ref;
      },
      { initialProps: { enabled: false } }
    );

    // Click while disabled
    outside.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    expect(handler).not.toHaveBeenCalled();

    // Enable and click
    rerender({ enabled: true });
    outside.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('stops listening when enabled becomes false', () => {
    const handler = vi.fn();

    const { rerender } = renderHook(
      ({ enabled }) => {
        const ref = useRef<HTMLDivElement>(target);
        useClickOutside(ref, handler, enabled);
        return ref;
      },
      { initialProps: { enabled: true } }
    );

    // Click while enabled
    outside.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    expect(handler).toHaveBeenCalledTimes(1);

    // Disable and click
    rerender({ enabled: false });
    outside.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    expect(handler).toHaveBeenCalledTimes(1); // Still 1, not called again
  });

  it('cleans up event listener on unmount', () => {
    const handler = vi.fn();
    const addSpy = vi.spyOn(document, 'addEventListener');
    const removeSpy = vi.spyOn(document, 'removeEventListener');

    const { unmount } = renderHook(() => {
      const ref = useRef<HTMLDivElement>(target);
      useClickOutside(ref, handler);
      return ref;
    });

    expect(addSpy).toHaveBeenCalledWith('mousedown', expect.any(Function));

    unmount();

    expect(removeSpy).toHaveBeenCalledWith('mousedown', expect.any(Function));
  });

  it('handles null ref gracefully', () => {
    const handler = vi.fn();

    renderHook(() => {
      const ref = useRef<HTMLDivElement>(null);
      useClickOutside(ref, handler);
      return ref;
    });

    // Should not crash when ref is null
    const event = new MouseEvent('mousedown', { bubbles: true });
    outside.dispatchEvent(event);

    // Handler should NOT be called when ref.current is null (guards against null)
    expect(handler).not.toHaveBeenCalled();
  });

  it('updates handler when it changes', () => {
    const handler1 = vi.fn();
    const handler2 = vi.fn();

    const { rerender } = renderHook(
      ({ handler }) => {
        const ref = useRef<HTMLDivElement>(target);
        useClickOutside(ref, handler);
        return ref;
      },
      { initialProps: { handler: handler1 } }
    );

    // Click with first handler
    outside.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    expect(handler1).toHaveBeenCalledTimes(1);
    expect(handler2).not.toHaveBeenCalled();

    // Change handler
    rerender({ handler: handler2 });

    // Click with second handler
    outside.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    expect(handler1).toHaveBeenCalledTimes(1); // Not called again
    expect(handler2).toHaveBeenCalledTimes(1);
  });
});
