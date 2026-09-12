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
  describe('a dialog is not "outside"', () => {
    /**
     * Every dropdown in this app opens its confirms through one shared
     * dialog, which is portalled to the body — so by the DOM's reckoning it
     * is outside every one of them. Without these three exemptions, answering
     * a confirm opened from inside a panel closes the panel underneath it, and
     * cancelling leaves the person nowhere near what they were doing.
     */
    function dialog(): { content: HTMLElement; overlay: HTMLElement } {
      const overlay = document.createElement('div');
      overlay.setAttribute('data-modal-overlay', '');
      const content = document.createElement('div');
      content.setAttribute('role', 'dialog');
      const button = document.createElement('button');
      content.appendChild(button);
      document.body.append(overlay, content);
      return { content: button, overlay };
    }

    it('ignores a click on the dialog itself', () => {
      const handler = vi.fn();
      const { content } = dialog();
      renderHook(() => {
        const ref = useRef<HTMLDivElement>(target);
        useClickOutside(ref, handler);
        return ref;
      });

      content.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      expect(handler).not.toHaveBeenCalled();
    });

    it('ignores a click on the dim backdrop behind it', () => {
      // The overlay is a sibling of the dialog with no role of its own, so
      // the role check alone does not cover it — and clicking the backdrop
      // is one of the three ways a person dismisses a confirm.
      const handler = vi.fn();
      const { overlay } = dialog();
      renderHook(() => {
        const ref = useRef<HTMLDivElement>(target);
        useClickOutside(ref, handler);
        return ref;
      });

      overlay.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      expect(handler).not.toHaveBeenCalled();
    });

    it('ignores the Escape that a dialog has already consumed', () => {
      // A dialog handles Escape on a capture listener and marks the event
      // handled, but does not stop it propagating. Without this the one
      // keypress closes both the confirm and the panel it was opened from.
      const handler = vi.fn();
      renderHook(() => {
        const ref = useRef<HTMLDivElement>(target);
        useClickOutside(ref, handler);
        return ref;
      });

      const consumed = new KeyboardEvent('keydown', { key: 'Escape', cancelable: true });
      document.addEventListener('keydown', (e) => e.preventDefault(), { capture: true, once: true });
      document.dispatchEvent(consumed);
      expect(handler).not.toHaveBeenCalled();

      // An Escape nobody consumed still closes the panel.
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', cancelable: true }));
      expect(handler).toHaveBeenCalledTimes(1);
    });
  });
});
