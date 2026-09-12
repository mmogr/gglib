import { useEffect, RefObject } from 'react';

/**
 * Hook that handles clicking outside of a referenced element.
 * Useful for closing dropdowns, modals, and menus when clicking outside.
 * 
 * @param ref - React ref object pointing to the element to monitor
 * @param handler - Callback function to execute when clicking outside
 * @param enabled - Whether the listener is active (default: true)
 * 
 * @example
 * ```tsx
 * const dropdownRef = useRef<HTMLDivElement>(null);
 * const [isOpen, setIsOpen] = useState(false);
 * 
 * useClickOutside(dropdownRef, () => setIsOpen(false), isOpen);
 * ```
 */
export function useClickOutside<T extends HTMLElement>(
  ref: RefObject<T | null>,
  handler: () => void,
  enabled: boolean = true
): void {
  useEffect(() => {
    if (!enabled) return;

    const handleClickOutside = (event: MouseEvent) => {
      const target = event.target as Node;
      // A dialog is "outside" every dropdown by construction — it is
      // portalled to the body — so without this, the first click in a
      // confirm opened from inside a panel closes the panel underneath it,
      // and cancelling leaves the person nowhere near what they were doing.
      //
      // A caller that *wants* the dropdown closed when it opens a dialog
      // should close it in the same handler rather than rely on this, which
      // is what `ProxyControl`'s View Dashboard now does: it used to close
      // by accident, and an accident is not a behaviour to inherit.
      if (
        target instanceof Element &&
        target.closest('[role="dialog"], [data-modal-overlay]')
      ) {
        return;
      }
      if (ref.current && !ref.current.contains(target)) {
        handler();
      }
    };

    const handleEscape = (event: KeyboardEvent) => {
      // A dialog consumes Escape on a capture listener and marks it handled,
      // but does not stop it propagating — so without this the same keypress
      // that closes a confirm also closes the panel it was opened from, which
      // is the keyboard half of the same bug the dialog check above fixes.
      if (event.defaultPrevented) return;
      if (event.key === 'Escape') {
        handler();
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    document.addEventListener('keydown', handleEscape);
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
      document.removeEventListener('keydown', handleEscape);
    };
  }, [ref, handler, enabled]);
}
