/**
 * The switcher at a branch point (ADR 0017): the previous and the next
 * option open their chats, and the count lists every option by its line,
 * the open chat's marked; choosing the open chat opens nothing. An option
 * that has nothing at the point yet says so.
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { BranchSwitcher } from '../../../src/components/ChatMessagesPanel/components/BranchSwitcher';
import type { BranchPoint } from '../../../src/types/generated/BranchPoint';

const POINT: BranchPoint = {
  message_id: 13,
  index: 1,
  options: [
    { conversation_id: 30, message_id: 23, role: 'user', preview: 'Make it cheaper' },
    { conversation_id: 20, message_id: 13, role: 'user', preview: 'Make it shorter' },
    { conversation_id: 40, message_id: null, role: null, preview: '' },
  ],
};

function renderAt(point: BranchPoint) {
  const open = vi.fn();
  render(<BranchSwitcher point={point} open={open} />);
  return open;
}

describe('BranchSwitcher', () => {
  it('says which option the chat is, of how many', () => {
    renderAt(POINT);
    expect(screen.getByRole('button', { name: 'Branch 2 of 3' })).toHaveTextContent('2/3');
  });

  it('opens the previous and the next option', async () => {
    const user = userEvent.setup();
    const open = renderAt(POINT);

    await user.click(screen.getByRole('button', { name: 'Previous branch' }));
    await user.click(screen.getByRole('button', { name: 'Next branch' }));

    expect(open.mock.calls).toEqual([[30], [40]]);
  });

  it('has no previous at the first option, nor a next at the last', () => {
    renderAt({ ...POINT, index: 0 });
    expect(screen.getByRole('button', { name: 'Previous branch' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Next branch' })).toBeEnabled();
  });

  it('lists every option by its line, the open chat marked, and opens the one chosen', async () => {
    const user = userEvent.setup();
    const open = renderAt(POINT);

    await user.click(screen.getByRole('button', { name: 'Branch 2 of 3' }));
    const list = screen.getByRole('group', { name: 'Branches here' });
    const options = within(list).getAllByRole('button');
    expect(options.map((o) => o.textContent)).toEqual(['Make it cheaper', 'Make it shorter', 'Nothing here yet']);
    expect(options[1]).toHaveAttribute('aria-current', 'true');

    await user.click(options[0]);
    expect(open).toHaveBeenCalledWith(30);
    expect(screen.queryByRole('group', { name: 'Branches here' })).not.toBeInTheDocument();
  });

  it('opens nothing for the open chat, and Escape closes the list', async () => {
    const user = userEvent.setup();
    const open = renderAt(POINT);

    await user.click(screen.getByRole('button', { name: 'Branch 2 of 3' }));
    await user.click(within(screen.getByRole('group', { name: 'Branches here' })).getByText('Make it shorter'));
    expect(open).not.toHaveBeenCalled();

    await user.click(screen.getByRole('button', { name: 'Branch 2 of 3' }));
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('group', { name: 'Branches here' })).not.toBeInTheDocument();
  });
});
