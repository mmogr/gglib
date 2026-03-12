import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import '@testing-library/jest-dom';
import React from 'react';
import { ToastProvider, useToastContext } from '../../../src/contexts/ToastContext';
import { ToastContainer } from '../../../src/components/Toast';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Renders the toast system with a control component that exposes showToast. */
function renderToastSystem() {
  let showToast!: ReturnType<typeof useToastContext>['showToast'];

  const Controls = () => {
    showToast = useToastContext().showToast;
    return null;
  };

  const { toasts: _toasts, ...rest } = { toasts: [] as ReturnType<typeof useToastContext>['toasts'] };

  const Wrapper = () => {
    const ctx = useToastContext();
    return (
      <>
        <Controls />
        <ToastContainer toasts={ctx.toasts} onDismiss={ctx.dismissToast} />
      </>
    );
  };

  const result = render(
    <ToastProvider>
      <Wrapper />
    </ToastProvider>
  );

  return { ...result, getShowToast: () => showToast };
}

// ---------------------------------------------------------------------------
// Stack limit
// ---------------------------------------------------------------------------

describe('ToastContext — stack limit', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('allows up to MAX_TOASTS (5) toasts without dropping any', () => {
    const { getShowToast } = renderToastSystem();
    act(() => {
      for (let i = 1; i <= 5; i++) getShowToast()(`Toast ${i}`, 'info');
    });
    expect(screen.getAllByRole('status')).toHaveLength(5);
  });

  it('marks the oldest toast as isDismissing when a 6th is added', () => {
    const { getShowToast } = renderToastSystem();
    act(() => {
      for (let i = 1; i <= 5; i++) getShowToast()(`Toast ${i}`, 'info');
    });

    // All 5 should be present
    const before = screen.getAllByText(/^Toast \d$/);
    expect(before).toHaveLength(5);

    // Adding a 6th should trigger exit on the oldest (Toast 1)
    act(() => {
      getShowToast()('Toast 6', 'info');
    });

    // After the exit animation (300ms) the oldest should be gone
    act(() => vi.advanceTimersByTime(400));

    expect(screen.queryByText('Toast 1')).not.toBeInTheDocument();
    expect(screen.getByText('Toast 6')).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Hover pause
// ---------------------------------------------------------------------------

describe('ToastItem — hover pauses auto-dismiss', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('does not dismiss while hovered', () => {
    const { getShowToast } = renderToastSystem();
    act(() => getShowToast()('Hover me', 'info', 1000));

    const toast = screen.getByText('Hover me').closest('[role="status"]')!;
    fireEvent.mouseEnter(toast);

    // Advance well past the natural duration — toast should still be there
    act(() => vi.advanceTimersByTime(3000));
    expect(screen.getByText('Hover me')).toBeInTheDocument();
  });

  it('dismisses after remaining duration on mouse leave', () => {
    const { getShowToast } = renderToastSystem();
    act(() => getShowToast()('Hover then leave', 'info', 1000));

    const toast = screen.getByText('Hover then leave').closest('[role="status"]')!;

    // Hover for 400ms then leave — 600ms remaining
    fireEvent.mouseEnter(toast);
    act(() => vi.advanceTimersByTime(400));
    fireEvent.mouseLeave(toast);

    // Should still be present just after leave
    act(() => vi.advanceTimersByTime(200));
    expect(screen.getByText('Hover then leave')).toBeInTheDocument();

    // Should be gone after the remaining ~600ms + animation
    act(() => vi.advanceTimersByTime(800));
    expect(screen.queryByText('Hover then leave')).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Keyboard — Escape dismissal
// ---------------------------------------------------------------------------

describe('ToastItem — keyboard', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('dismisses on Escape key press', () => {
    const { getShowToast } = renderToastSystem();
    act(() => getShowToast()('Press Escape', 'info'));

    const toast = screen.getByText('Press Escape').closest('[role="status"]')!;
    fireEvent.keyDown(toast, { key: 'Escape' });

    // Exit animation plays for 300ms then it's removed
    act(() => vi.advanceTimersByTime(400));
    expect(screen.queryByText('Press Escape')).not.toBeInTheDocument();
  });

  it('does not dismiss on other key presses', () => {
    const { getShowToast } = renderToastSystem();
    act(() => getShowToast()('Other key', 'info', 10000));

    const toast = screen.getByText('Other key').closest('[role="status"]')!;
    fireEvent.keyDown(toast, { key: 'Enter' });

    act(() => vi.advanceTimersByTime(500));
    expect(screen.getByText('Other key')).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// ARIA roles
// ---------------------------------------------------------------------------

describe('ToastItem — ARIA roles', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it.each([
    ['error', 'alert'],
    ['warning', 'alert'],
    ['success', 'status'],
    ['info', 'status'],
  ] as const)('%s toast has role="%s"', (type, expectedRole) => {
    const { getShowToast } = renderToastSystem();
    act(() => getShowToast()(`A ${type} message`, type as 'error' | 'warning' | 'success' | 'info'));
    expect(screen.getByRole(expectedRole)).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Click target — only close button dismisses
// ---------------------------------------------------------------------------

describe('ToastItem — click target', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('does not dismiss when clicking the toast body', () => {
    const { getShowToast } = renderToastSystem();
    act(() => getShowToast()('Body click', 'info', 10000));

    const messageSpan = screen.getByText('Body click');
    fireEvent.click(messageSpan);

    act(() => vi.advanceTimersByTime(500));
    expect(screen.getByText('Body click')).toBeInTheDocument();
  });

  it('dismisses when clicking the close button', () => {
    const { getShowToast } = renderToastSystem();
    act(() => getShowToast()('Close button', 'info'));

    const closeBtn = screen.getByRole('button', { name: /dismiss notification/i });
    fireEvent.click(closeBtn);

    act(() => vi.advanceTimersByTime(400));
    expect(screen.queryByText('Close button')).not.toBeInTheDocument();
  });
});
