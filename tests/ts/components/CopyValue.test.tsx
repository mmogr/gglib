/**
 * ⧉ puts the whole value on the clipboard, says "Copied" beside itself for
 * one second once it is there, tells its caller, and does not hand the
 * click to the row it sits in.
 */

import { describe, it, expect, vi, afterEach } from 'vitest';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { CopyValue } from '../../../src/components/CopyValue';
import { appLogger } from '../../../src/services/platform';

describe('CopyValue', () => {
  it('is named by its label and copies the whole value, then says so', async () => {
    const onCopied = vi.fn();
    // `userEvent.setup()` gives the page a clipboard a test can read back.
    const user = userEvent.setup();
    render(<CopyValue value="http://127.0.0.1:8080/v1" label="Copy the endpoint address" onCopied={onCopied} />);
    await user.click(screen.getByRole('button', { name: 'Copy the endpoint address' }));

    expect(await navigator.clipboard.readText()).toBe('http://127.0.0.1:8080/v1');
    await waitFor(() => expect(onCopied).toHaveBeenCalledWith('http://127.0.0.1:8080/v1'));
  });

  describe('Copied', () => {
    afterEach(() => vi.useRealTimers());

    it('is said beside ⧉ once the value is there, for one second', async () => {
      vi.useFakeTimers({ shouldAdvanceTime: true });
      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
      render(<CopyValue value="Qwen3-4B" label="Copy the Model ID" />);
      expect(screen.queryByText('Copied')).not.toBeInTheDocument();
      await user.click(screen.getByRole('button', { name: 'Copy the Model ID' }));

      expect(await screen.findByRole('status')).toHaveTextContent('Copied');
      act(() => vi.advanceTimersByTime(900));
      expect(screen.getByText('Copied')).toBeInTheDocument();
      act(() => vi.advanceTimersByTime(150));
      expect(screen.queryByText('Copied')).not.toBeInTheDocument();
    });

    it('a second copy while it is up keeps it up for a second from then', async () => {
      vi.useFakeTimers({ shouldAdvanceTime: true });
      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
      render(<CopyValue value="Qwen3-4B" label="Copy the Model ID" />);
      await user.click(screen.getByRole('button', { name: 'Copy the Model ID' }));
      await screen.findByText('Copied');
      act(() => vi.advanceTimersByTime(700));
      await user.click(screen.getByRole('button', { name: 'Copy the Model ID' }));
      act(() => vi.advanceTimersByTime(700));
      expect(screen.getByText('Copied')).toBeInTheDocument();
      act(() => vi.advanceTimersByTime(400));
      expect(screen.queryByText('Copied')).not.toBeInTheDocument();
    });

    it('is not said when the copy failed, which is logged', async () => {
      const user = userEvent.setup();
      const complaints = vi.spyOn(appLogger, 'error').mockImplementation(() => {});
      const refused = new Error('clipboard refused');
      vi.spyOn(navigator.clipboard, 'writeText').mockRejectedValueOnce(refused);
      try {
        render(<CopyValue value="x" label="Copy x" />);
        await user.click(screen.getByRole('button', { name: 'Copy x' }));
        await waitFor(() => expect(complaints).toHaveBeenCalledWith('component.app', 'Failed to copy to clipboard', { error: refused }));
        expect(screen.queryByText('Copied')).not.toBeInTheDocument();
      } finally {
        complaints.mockRestore();
      }
    });
  });

  it('keeps the click from the row it sits in', async () => {
    const rowClick = vi.fn();
    render(
      <div onClick={rowClick}>
        <CopyValue value="x" label="Copy x" />
      </div>,
    );
    await userEvent.setup().click(screen.getByRole('button', { name: 'Copy x' }));
    expect(rowClick).not.toHaveBeenCalled();
  });
});
