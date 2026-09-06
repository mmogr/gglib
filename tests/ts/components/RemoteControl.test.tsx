/**
 * The Remote popover, both halves.
 *
 * What is pinned here is the shape of the one-time reveal — `enable`'s answer
 * is the only time the ticket and the code exist on a screen, so the panel
 * has to show them the moment they arrive and nowhere else — and the
 * connect half's guard rails: the button is dead with nothing to dial, and
 * a stored ticket without a stored key says so instead of failing later.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { ReactNode } from 'react';

import RemoteControl from '../../../src/components/RemoteControl';
import { ToastProvider } from '../../../src/contexts/ToastContext';
import { ConfirmProvider } from '../../../src/contexts/ConfirmContext';
import {
  IDLE_STATUS,
  applyRemoteStatus,
  resetRemoteState,
} from '../../../src/services/remoteRegistry';

const enableRemote = vi.fn();
const connectRemote = vi.fn();

vi.mock('../../../src/services/transport', () => ({
  getTransport: () => ({ enableRemote, connectRemote }),
}));
vi.mock('../../../src/services/remoteEvents', () => ({
  refreshRemoteStatus: vi.fn(),
}));

const wrapper = ({ children }: { children: ReactNode }) => (
  <ToastProvider>
    <ConfirmProvider>{children}</ConfirmProvider>
  </ToastProvider>
);

const TICKET = 'pipeadlvvgabqkyqvn6vjp7nhslea45a5yls6pnkmizfv4bbu2hxa5iruaaauhlp2na';

async function open() {
  render(<RemoteControl />, { wrapper });
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: /remote/i }));
  return user;
}

describe('RemoteControl', () => {
  beforeEach(() => {
    resetRemoteState();
    applyRemoteStatus(IDLE_STATUS);
    enableRemote.mockReset();
    connectRemote.mockReset();
  });

  it('shows both halves, off, with the /mcp grant unchecked', async () => {
    await open();
    expect(screen.getByRole('heading', { name: 'This machine' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Another machine' })).toBeInTheDocument();
    expect(screen.getByRole('checkbox', { name: /reach \/mcp/i })).not.toBeChecked();
  });

  it('enable reveals the code and the pairing string exactly as the daemon answered', async () => {
    enableRemote.mockResolvedValue({
      ticket: TICKET,
      code: '483920',
      pairing: `${TICKET}-483920`,
      expires_in_s: 120,
    });
    const user = await open();
    await user.click(screen.getByRole('button', { name: /enable remote access/i }));

    expect(enableRemote).toHaveBeenCalledWith({ allow_mcp: false });
    expect(await screen.findByText('483920')).toBeInTheDocument();
    expect(screen.getByText(`gglib remote connect ${TICKET}-483920`)).toBeInTheDocument();
  });

  it('connect is dead with nothing to dial, and alive once a string is typed', async () => {
    const user = await open();
    const connect = screen.getByRole('button', { name: /^connect$/i });
    expect(connect).toBeDisabled();

    await user.type(screen.getByLabelText(/pairing string/i), `${TICKET}-483920`);
    expect(connect).toBeEnabled();

    connectRemote.mockResolvedValue({
      port: 41234,
      base_url: 'http://127.0.0.1:41234/v1',
      ticket_fingerprint: '3ca82708b995',
      paired: true,
    });
    await user.click(connect);
    expect(connectRemote).toHaveBeenCalledWith({ pairing: `${TICKET}-483920` });
  });

  it('a remembered ticket without a key is named as the problem', async () => {
    applyRemoteStatus({ ...IDLE_STATUS, stored_ticket_fingerprint: '3ca82708b995', has_remote_key: false });
    await open();
    expect(screen.getByText(/no key is stored/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^connect$/i })).toBeDisabled();
  });

  it('a remembered pairing lets connect dial it with an empty box', async () => {
    applyRemoteStatus({ ...IDLE_STATUS, stored_ticket_fingerprint: '3ca82708b995', has_remote_key: true });
    connectRemote.mockResolvedValue({
      port: 41234,
      base_url: 'http://127.0.0.1:41234/v1',
      ticket_fingerprint: '3ca82708b995',
      paired: false,
    });
    const user = await open();
    await user.click(screen.getByRole('button', { name: /^connect$/i }));
    expect(connectRemote).toHaveBeenCalledWith({});
  });
});
