/**
 * `SecretField`: a stored secret held, not shown, and revealed only when
 * Show or Copy asks (ADR 0027).
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { useState } from 'react';

import {
  KEEP,
  MASK,
  SecretField,
  editedValue,
  type SecretEdit,
} from '../../../src/components/ui/SecretField';
import type { SecretRef } from '../../../src/types/generated/SecretRef';

const { revealSecret } = vi.hoisted(() => ({ revealSecret: vi.fn() }));

vi.mock('../../../src/services/transport', () => ({
  getTransport: () => ({ revealSecret }),
}));

const SECRET: SecretRef = { kind: 'mcp_env', server_id: 4, key: 'TOKEN' };

/** The field, holding its own edit, and the last edit it handed back. */
function Harness({
  isSet,
  clearable,
  onNotice,
  edits,
  secret = SECRET,
}: {
  isSet: boolean;
  clearable?: boolean;
  onNotice?: (message: string, kind: 'success' | 'error') => void;
  edits: SecretEdit[];
  secret?: SecretRef;
}) {
  const [edit, setEdit] = useState<SecretEdit>(KEEP);
  return (
    <SecretField
      id="token"
      label="TOKEN value"
      isSet={isSet}
      secret={secret}
      edit={edit}
      onEdit={(next) => {
        edits.push(next);
        setEdit(next);
      }}
      clearable={clearable}
      clearNote="Gone for good."
      onNotice={onNotice}
    />
  );
}

describe('SecretField', () => {
  beforeEach(() => {
    revealSecret.mockReset();
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('shows a held secret as the mask and asks for nothing', () => {
    render(<Harness isSet edits={[]} />);

    const held = screen.getByLabelText('TOKEN value');
    expect(held).toHaveValue(MASK);
    expect(held).toHaveAttribute('readonly');
    expect(revealSecret).not.toHaveBeenCalled();
  });

  it('Show reveals the value in place, and Hide puts the mask back', async () => {
    revealSecret.mockResolvedValue('t0k-81d0');
    const user = userEvent.setup();
    render(<Harness isSet edits={[]} />);

    await user.click(screen.getByRole('button', { name: 'Show the TOKEN value' }));

    expect(revealSecret).toHaveBeenCalledWith(SECRET);
    await waitFor(() => expect(screen.getByLabelText('TOKEN value')).toHaveValue('t0k-81d0'));

    await user.click(screen.getByRole('button', { name: 'Hide the TOKEN value' }));
    expect(screen.getByLabelText('TOKEN value')).toHaveValue(MASK);
  });

  /** A field reused for another secret, as a list row is, shows its mask. */
  it('puts the mask back when the field is handed another secret', async () => {
    revealSecret.mockResolvedValue('t0k-81d0');
    const user = userEvent.setup();
    const { rerender } = render(<Harness isSet edits={[]} />);
    await user.click(screen.getByRole('button', { name: 'Show the TOKEN value' }));
    await waitFor(() => expect(screen.getByLabelText('TOKEN value')).toHaveValue('t0k-81d0'));

    rerender(<Harness isSet edits={[]} secret={{ kind: 'mcp_env', server_id: 4, key: 'ROOT' }} />);

    expect(screen.getByLabelText('TOKEN value')).toHaveValue(MASK);
  });

  it('puts the mask back after Clear and Undo', async () => {
    revealSecret.mockResolvedValue('t0k-81d0');
    const user = userEvent.setup();
    render(<Harness isSet clearable edits={[]} />);
    await user.click(screen.getByRole('button', { name: 'Show the TOKEN value' }));
    await waitFor(() => expect(screen.getByLabelText('TOKEN value')).toHaveValue('t0k-81d0'));

    await user.click(screen.getByRole('button', { name: 'Clear' }));
    await user.click(screen.getByRole('button', { name: 'Undo' }));

    expect(screen.getByLabelText('TOKEN value')).toHaveValue(MASK);
  });

  it('Copy writes the revealed value, and says so only once the write is done', async () => {
    revealSecret.mockResolvedValue('t0k-81d0');
    let finish: () => void = () => {};
    const written = new Promise<void>((resolve) => {
      finish = resolve;
    });
    const items: { data: Record<string, Promise<Blob>> }[] = [];
    vi.stubGlobal(
      'ClipboardItem',
      class {
        constructor(public data: Record<string, Promise<Blob>>) {
          items.push(this);
        }
      },
    );
    const write = vi.fn(() => written);
    const user = userEvent.setup();
    Object.defineProperty(navigator, 'clipboard', { value: { write }, configurable: true });
    const onNotice = vi.fn();
    render(<Harness isSet edits={[]} onNotice={onNotice} />);

    await user.click(screen.getByRole('button', { name: 'Copy the TOKEN value' }));

    expect(write).toHaveBeenCalledTimes(1);
    expect(revealSecret).toHaveBeenCalledWith(SECRET);
    const blob = await items[0].data['text/plain'];
    const text = await new Promise<string>((resolve) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result as string);
      reader.readAsText(blob);
    });
    expect(text).toBe('t0k-81d0');
    expect(onNotice).not.toHaveBeenCalled();

    finish();
    await waitFor(() => expect(onNotice).toHaveBeenCalledWith('Copied the TOKEN value.', 'success'));
  });

  it('Replace opens an empty input, and what is typed is the edit', async () => {
    const edits: SecretEdit[] = [];
    const user = userEvent.setup();
    render(<Harness isSet edits={edits} />);

    await user.click(screen.getByRole('button', { name: 'Replace' }));
    const input = screen.getByLabelText('TOKEN value');
    expect(input).toHaveValue('');
    expect(input).toHaveAttribute('type', 'password');
    expect(input).toHaveAttribute('placeholder', 'Leave empty to keep the current TOKEN value');
    await user.click(screen.getByRole('button', { name: 'Show the typed TOKEN value' }));
    expect(input).toHaveAttribute('type', 'text');
    await user.type(input, 'new');

    expect(edits.at(-1)).toEqual({ kind: 'replace', value: 'new' });
    expect(editedValue(edits.at(-1)!)).toBe('new');

    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(edits.at(-1)).toEqual(KEEP);
    expect(screen.getByLabelText('TOKEN value')).toHaveValue(MASK);
  });

  it('Clear is offered only where allowed, says what it does, and can be undone', async () => {
    const edits: SecretEdit[] = [];
    const user = userEvent.setup();
    const { unmount } = render(<Harness isSet edits={edits} />);
    expect(screen.queryByRole('button', { name: 'Clear' })).not.toBeInTheDocument();
    unmount();

    render(<Harness isSet clearable edits={edits} />);
    await user.click(screen.getByRole('button', { name: 'Clear' }));

    expect(edits.at(-1)).toEqual({ kind: 'clear' });
    expect(editedValue(edits.at(-1)!)).toBeNull();
    expect(screen.getByText('Gone for good.')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Undo' }));
    expect(edits.at(-1)).toEqual(KEEP);
  });

  it('with nothing stored is the input alone, and an empty input keeps', async () => {
    const edits: SecretEdit[] = [];
    const user = userEvent.setup();
    render(<Harness isSet={false} edits={edits} />);

    expect(screen.queryByRole('button', { name: /copy/i })).not.toBeInTheDocument();
    const input = screen.getByLabelText('TOKEN value');
    await user.type(input, 'x');
    await user.clear(input);

    expect(editedValue(edits.at(-1)!)).toBeUndefined();
    expect(editedValue(KEEP)).toBeUndefined();
    expect(editedValue({ kind: 'replace', value: '   ' })).toBeUndefined();
  });

  /** Every secret input sits in some form; its eye must never send it. */
  it('the eye shows what was typed and does not submit the form around it', async () => {
    const submitted = vi.fn((event: { preventDefault: () => void }) => event.preventDefault());
    const user = userEvent.setup();
    render(
      <form onSubmit={submitted}>
        <Harness isSet={false} edits={[]} />
      </form>,
    );
    const input = screen.getByLabelText('TOKEN value');
    expect(input).toHaveAttribute('type', 'password');

    await user.click(screen.getByRole('button', { name: 'Show the typed TOKEN value' }));

    expect(input).toHaveAttribute('type', 'text');
    expect(submitted).not.toHaveBeenCalled();
  });
});
