/**
 * ⧉ puts the whole value on the clipboard, tells its caller once it is
 * there, and does not hand the click to the row it sits in.
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { CopyValue } from '../../../src/components/CopyValue';

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
