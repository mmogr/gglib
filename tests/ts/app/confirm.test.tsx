/**
 * The confirmation question as a store: asked from outside React, drawn by
 * the provider, answered once, and never two at a time.
 */

import { describe, it, expect } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

import { confirm, dismissConfirm } from '../../../src/app/confirm';
import { ConfirmProvider, useConfirmContext } from '../../../src/contexts/ConfirmContext';

function Asker() {
  const { confirm: ask } = useConfirmContext();
  return <button onClick={() => void ask({ title: 'Remove "qwen3"?' })}>remove</button>;
}

describe('the confirm store', () => {
  it('a question asked outside React is drawn by the provider and answered by its buttons', async () => {
    render(<ConfirmProvider><div /></ConfirmProvider>);
    let answer: boolean | null = null;
    act(() => void confirm({ title: 'Remove "qwen3"?', confirmLabel: 'Remove' }).then((v) => (answer = v)));

    expect(await screen.findByRole('heading', { name: 'Remove "qwen3"?' })).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Remove' }));

    await act(async () => {});
    expect(answer).toBe(true);
    expect(screen.queryByRole('heading', { name: 'Remove "qwen3"?' })).not.toBeInTheDocument();
  });

  it('the context hands components the same confirm, and Cancel answers false', async () => {
    render(<ConfirmProvider><Asker /></ConfirmProvider>);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'remove' }));
    expect(await screen.findByRole('heading', { name: 'Remove "qwen3"?' })).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /cancel/i }));

    expect(screen.queryByRole('heading', { name: 'Remove "qwen3"?' })).not.toBeInTheDocument();
  });

  it('a second question while one is open is answered false at once, and the first stands', async () => {
    const first = confirm({ title: 'First?' });
    await expect(confirm({ title: 'Second?' })).resolves.toBe(false);

    dismissConfirm();
    await expect(first).resolves.toBe(false);
  });

  it('a move through history answers the open question false', async () => {
    render(<ConfirmProvider><div /></ConfirmProvider>);
    const asked = confirm({ title: 'Going?' });
    expect(await screen.findByRole('heading', { name: 'Going?' })).toBeInTheDocument();

    act(() => window.dispatchEvent(new PopStateEvent('popstate')));

    await expect(asked).resolves.toBe(false);
  });
});
