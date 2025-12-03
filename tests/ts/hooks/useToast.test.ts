/**
 * Tests for useToast hook.
 * 
 * Tests toast notification state management.
 */

import { describe, it, expect } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useToast } from '../../../src/hooks/useToast';

describe('useToast', () => {
  describe('initial state', () => {
    it('starts with empty toasts array', () => {
      const { result } = renderHook(() => useToast());

      expect(result.current.toasts).toEqual([]);
    });
  });

  describe('showToast', () => {
    it('adds toast with default type "info"', () => {
      const { result } = renderHook(() => useToast());

      act(() => {
        result.current.showToast('Test message');
      });

      expect(result.current.toasts).toHaveLength(1);
      expect(result.current.toasts[0]).toMatchObject({
        message: 'Test message',
        type: 'info',
      });
      expect(result.current.toasts[0].id).toBeDefined();
    });

    it('adds toast with specified type', () => {
      const { result } = renderHook(() => useToast());

      act(() => {
        result.current.showToast('Success!', 'success');
      });

      expect(result.current.toasts[0].type).toBe('success');
    });

    it('adds toast with custom duration', () => {
      const { result } = renderHook(() => useToast());

      act(() => {
        result.current.showToast('Quick toast', 'info', 2000);
      });

      expect(result.current.toasts[0].duration).toBe(2000);
    });

    it('adds multiple toasts', () => {
      const { result } = renderHook(() => useToast());

      act(() => {
        result.current.showToast('First', 'info');
        result.current.showToast('Second', 'success');
        result.current.showToast('Third', 'error');
      });

      expect(result.current.toasts).toHaveLength(3);
      expect(result.current.toasts[0].message).toBe('First');
      expect(result.current.toasts[1].message).toBe('Second');
      expect(result.current.toasts[2].message).toBe('Third');
    });

    it('generates unique IDs for each toast', () => {
      const { result } = renderHook(() => useToast());

      act(() => {
        result.current.showToast('First');
        result.current.showToast('Second');
      });

      const ids = result.current.toasts.map((t) => t.id);
      expect(new Set(ids).size).toBe(2);
    });
  });

  describe('dismissToast', () => {
    it('removes toast by ID', () => {
      const { result } = renderHook(() => useToast());

      act(() => {
        result.current.showToast('Toast 1');
        result.current.showToast('Toast 2');
      });

      const firstId = result.current.toasts[0].id;

      act(() => {
        result.current.dismissToast(firstId);
      });

      expect(result.current.toasts).toHaveLength(1);
      expect(result.current.toasts[0].message).toBe('Toast 2');
    });

    it('does nothing for non-existent ID', () => {
      const { result } = renderHook(() => useToast());

      act(() => {
        result.current.showToast('Test');
      });

      act(() => {
        result.current.dismissToast('non-existent-id');
      });

      expect(result.current.toasts).toHaveLength(1);
    });
  });

  describe('clearAllToasts', () => {
    it('removes all toasts', () => {
      const { result } = renderHook(() => useToast());

      act(() => {
        result.current.showToast('First');
        result.current.showToast('Second');
        result.current.showToast('Third');
      });

      expect(result.current.toasts).toHaveLength(3);

      act(() => {
        result.current.clearAllToasts();
      });

      expect(result.current.toasts).toEqual([]);
    });

    it('is safe to call when empty', () => {
      const { result } = renderHook(() => useToast());

      act(() => {
        result.current.clearAllToasts();
      });

      expect(result.current.toasts).toEqual([]);
    });
  });

  describe('toast types', () => {
    it('supports info type', () => {
      const { result } = renderHook(() => useToast());

      act(() => {
        result.current.showToast('Info', 'info');
      });

      expect(result.current.toasts[0].type).toBe('info');
    });

    it('supports success type', () => {
      const { result } = renderHook(() => useToast());

      act(() => {
        result.current.showToast('Success', 'success');
      });

      expect(result.current.toasts[0].type).toBe('success');
    });

    it('supports error type', () => {
      const { result } = renderHook(() => useToast());

      act(() => {
        result.current.showToast('Error', 'error');
      });

      expect(result.current.toasts[0].type).toBe('error');
    });

    it('supports warning type', () => {
      const { result } = renderHook(() => useToast());

      act(() => {
        result.current.showToast('Warning', 'warning');
      });

      expect(result.current.toasts[0].type).toBe('warning');
    });
  });
});
