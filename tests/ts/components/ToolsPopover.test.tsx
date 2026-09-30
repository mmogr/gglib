/**
 * Which way the Tools popout grows.
 *
 * It lined up with the button's right edge and grew leftwards, which suited
 * a button at the right of a header. The notebook moved the button into the
 * composer's left margin, where growing leftwards put the popout off the
 * page; the composer's case is in `ChatPageNotebook.test.tsx`. This is the
 * default, which must still grow leftwards.
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

import { ToolsPopover } from '../../../src/components/ToolsPopover';

describe('ToolsPopover', () => {
  it('lines up with the right edge by default, and caps its height', async () => {
    render(<ToolsPopover />);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Tools' }));

    const popout = screen.getByText(/active$/).closest('.z-popover');
    expect(popout).toHaveClass('right-0');
    expect(popout).not.toHaveClass('left-0');
    expect(popout).toHaveClass('max-h-[70vh]', 'overflow-y-auto');
  });
});
