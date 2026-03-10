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
      if (ref.current && !ref.current.contains(event.target as Node)) {
        handler();
      }
    };

    const handleEscape = (event: KeyboardEvent) => {
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
