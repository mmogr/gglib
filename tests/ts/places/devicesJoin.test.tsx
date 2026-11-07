/**
 * The Devices place's Use another machine, at `/devices`: Join with a
 * string, trimmed on send, "Reaching it…" in place and a refusal shown as it
 * came; Reconnect only with a remembered pairing and key, and the line for
 * one without a key; joined, the machine by its name and the reach word from
 * the status, never its fingerprint, its address, the command that prints
 * the key (no Show), where its models are; Away since the time it went;
 * Disconnect and the line after it with Reconnect; ⋯ Stop desk's gglib…
 * asking first.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

const transport = vi.hoisted(() => ({
  joinRemote: vi.fn(),
  disconnectRemote: vi.fn(),
  killRemote: vi.fn(),
  disableRemote: vi.fn(),
  inviteRemote: vi.fn(),
}));
const refresh = vi.hoisted(() => vi.fn(async () => true));

vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return {
    ...actual,
    getTransport: () => ({
      ...transport,
      getSettings: vi.fn(async () => ({})),
      subscribe: vi.fn(() => () => {}),
      onEventStreamOpen: vi.fn(() => () => {}),
    }),
  };
});
vi.mock('../../../src/services/remoteEvents', () => ({ refreshRemoteStatus: () => refresh() }));

import { IDLE_STATUS, applyRemoteStatus, ingestRemoteEvent, resetRemoteState } from '../../../src/services/remoteRegistry';
import { renderApp } from './harness';
import { DESK, FINGERPRINT, SHARED, TICKET, fingerprintShown } from './devicesFixtures';

const field = () => screen.getByRole('textbox', { name: 'Paste a pairing string' });
const join = () => screen.getByRole('button', { name: 'Join' });
const REMEMBERED = { ...IDLE_STATUS, stored_ticket_fingerprint: FINGERPRINT, paired_name: 'desk', has_remote_key: true };

describe('Use another machine', () => {
  beforeEach(() => {
    resetRemoteState();
    Object.values(transport).forEach((fn) => fn.mockReset());
    refresh.mockReset().mockResolvedValue(true);
  });
  afterEach(() => vi.useRealTimers());

  it('Join is dead with nothing typed, and sends the string trimmed, saying "Reaching it…" in place', async () => {
    applyRemoteStatus(IDLE_STATUS);
    let answer: (v: unknown) => void = () => {};
    transport.joinRemote.mockImplementation(() => new Promise((resolve) => (answer = resolve)));
    renderApp('/devices');
    expect(join()).toBeDisabled();
    const user = userEvent.setup();
    await user.type(field(), `  ${TICKET}-483920 `);
    await user.click(join());
    expect(transport.joinRemote).toHaveBeenCalledWith({ pairing: `${TICKET}-483920` });
    expect(screen.getByText('Reaching it…')).toBeInTheDocument();
    await act(async () => answer({}));
    expect(field()).toHaveValue('');
  });

  it('a refused join is shown as the daemon said it', async () => {
    applyRemoteStatus(IDLE_STATUS);
    transport.joinRemote.mockRejectedValue(new Error('the pairing code was already used'));
    renderApp('/devices');
    const user = userEvent.setup();
    await user.type(field(), `${TICKET}-483920`);
    await user.click(join());
    expect(await screen.findByText('the pairing code was already used')).toBeInTheDocument();
  });

  it('offers Reconnect only with a remembered pairing and key, and dials it with no string', async () => {
    applyRemoteStatus(REMEMBERED);
    renderApp('/devices');
    expect(screen.getByText(/Last joined desk/)).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Reconnect' }));
    expect(transport.joinRemote).toHaveBeenCalledWith({});
    expect(fingerprintShown()).toBe(false);
  });

  it('a remembered pairing with no key says so and offers no Reconnect', () => {
    applyRemoteStatus({ ...REMEMBERED, has_remote_key: false });
    renderApp('/devices');
    expect(screen.getByText(/Last joined desk, but no key is stored here/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Reconnect' })).not.toBeInTheDocument();
  });

  it('nothing remembered offers no Reconnect', () => {
    applyRemoteStatus(IDLE_STATUS);
    renderApp('/devices');
    expect(screen.queryByRole('button', { name: 'Reconnect' })).not.toBeInTheDocument();
    expect(screen.queryByText(/Last joined/)).not.toBeInTheDocument();
  });

  it("joined, shows the machine's name and the status's reach word, never its fingerprint", () => {
    applyRemoteStatus(DESK);
    renderApp('/devices');
    expect(screen.getByText('desk · Connected · direct')).toBeInTheDocument();
    expect(screen.getByText('http://127.0.0.1:41234/v1')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Copy the address' })).toBeInTheDocument();
    expect(screen.getByText("desk's models are in your Library.")).toBeInTheDocument();
    expect(fingerprintShown()).toBe(false);
  });

  it('takes the reach word from the status, not from the connection', () => {
    applyRemoteStatus({ ...DESK, reach: 'disconnected' });
    renderApp('/devices');
    expect(screen.getByText('desk · Disconnected · direct')).toBeInTheDocument();
  });

  it('names a machine that gave no name in words', () => {
    applyRemoteStatus({ ...DESK, paired_name: null });
    renderApp('/devices');
    expect(screen.getByText('the paired machine · Connected · direct')).toBeInTheDocument();
  });

  it('a connection that has not named its peer yet names nobody', () => {
    applyRemoteStatus({ ...DESK, stored_ticket_fingerprint: 'aabbccddeeff', connected: { ...DESK.connected!, ticket_fingerprint: '' } });
    renderApp('/devices');
    expect(screen.getByText('Connected. Reading which machine…')).toBeInTheDocument();
  });

  it("points at the command that prints this device's key, and offers no Show", () => {
    applyRemoteStatus(DESK);
    renderApp('/devices');
    expect(screen.getByText("Clients need this device's key:")).toBeInTheDocument();
    expect(screen.getByText('gglib config settings reveal remote-pairing.api-key')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Copy the command that prints the key' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /show/i })).not.toBeInTheDocument();
  });

  it('away, says since when and that it reconnects on its own', () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    vi.setSystemTime(new Date(2026, 10, 7, 14, 32, 0));
    applyRemoteStatus({ ...DESK, reach: 'away', connected: { ...DESK.connected!, away_for_s: 30 * 60 } });
    renderApp('/devices');
    expect(screen.getByText('desk · Away since 14:02 · reconnects on its own')).toBeInTheDocument();
  });

  it('Disconnect, then says the pairing is remembered with Reconnect', async () => {
    applyRemoteStatus(DESK);
    transport.disconnectRemote.mockResolvedValue(IDLE_STATUS);
    renderApp('/devices');
    await userEvent.setup().click(screen.getByRole('button', { name: 'Disconnect' }));
    expect(transport.disconnectRemote).toHaveBeenCalled();
    act(() => applyRemoteStatus(REMEMBERED));
    expect(screen.getByText(/Disconnected\. The pairing is remembered\./)).toBeInTheDocument();
    expect(screen.queryByText(/Last joined/)).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Reconnect' })).toBeInTheDocument();
  });

  it("⋯ Stop desk's gglib… names what stops, and only the confirm sends it", async () => {
    applyRemoteStatus(DESK);
    transport.killRemote.mockResolvedValue(IDLE_STATUS);
    renderApp('/devices');
    const user = userEvent.setup();
    const open = async () => {
      await user.click(screen.getByRole('button', { name: 'More for desk' }));
      await user.click(screen.getByRole('menuitem', { name: "Stop desk's gglib…" }));
    };
    await open();
    expect(await screen.findByText("Stop desk's gglib?")).toBeInTheDocument();
    expect(screen.getByText(/This stops gglib on desk: its endpoint, its models, its downloads/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(transport.killRemote).not.toHaveBeenCalled();
    await open();
    await user.click(await screen.findByRole('button', { name: 'Stop it' }));
    expect(transport.killRemote).toHaveBeenCalled();
  });

  it('the joined line follows away and back at once, with no read (DV-4)', () => {
    applyRemoteStatus(DESK);
    renderApp('/devices');
    act(() => ingestRemoteEvent({ type: 'remote_away', port: 41234 }));
    expect(screen.getByText(/^desk · Away since \d\d:\d\d · reconnects on its own$/)).toBeInTheDocument();
    act(() => ingestRemoteEvent({ type: 'remote_back', port: 41234 }));
    expect(screen.getByText('desk · Connected · direct')).toBeInTheDocument();
  });

  it('a refused Disconnect is said in the joined view', async () => {
    applyRemoteStatus(DESK);
    transport.disconnectRemote.mockRejectedValue(new Error('the tunnel is busy'));
    renderApp('/devices');
    await userEvent.setup().click(screen.getByRole('button', { name: 'Disconnect' }));
    expect(await screen.findByText('the tunnel is busy')).toBeInTheDocument();
  });

  it('a connection that drops with no key stored offers no Reconnect', () => {
    applyRemoteStatus({ ...DESK, has_remote_key: false });
    renderApp('/devices');
    act(() => applyRemoteStatus({ ...REMEMBERED, has_remote_key: false }));
    expect(screen.queryByText(/The pairing is remembered/)).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Reconnect' })).not.toBeInTheDocument();
  });

  it('"Reaching it…" stands in for Reconnect while it dials', async () => {
    applyRemoteStatus(REMEMBERED);
    transport.joinRemote.mockImplementation(() => new Promise(() => {}));
    renderApp('/devices');
    await userEvent.setup().click(screen.getByRole('button', { name: 'Reconnect' }));
    expect(screen.getByText('Reaching it…')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Reconnect' })).not.toBeInTheDocument();
  });

  it('a join refusal goes when the field changes', async () => {
    applyRemoteStatus(IDLE_STATUS);
    transport.joinRemote.mockRejectedValue(new Error('the pairing code was already used'));
    renderApp('/devices');
    const user = userEvent.setup();
    await user.type(field(), `${TICKET}-483920`);
    await user.click(join());
    await screen.findByText('the pairing code was already used');
    await user.type(field(), 'x');
    expect(screen.queryByText('the pairing code was already used')).not.toBeInTheDocument();
  });

  it('a join refusal goes when another action on the page runs', async () => {
    applyRemoteStatus(SHARED);
    transport.joinRemote.mockRejectedValue(new Error('the pairing code was already used'));
    transport.disableRemote.mockResolvedValue(IDLE_STATUS);
    renderApp('/devices');
    const user = userEvent.setup();
    await user.type(field(), `${TICKET}-483920`);
    await user.click(join());
    await screen.findByText('the pairing code was already used');
    await user.click(screen.getByRole('button', { name: 'Stop sharing' }));
    expect(screen.queryByText('the pairing code was already used')).not.toBeInTheDocument();
  });
  it("a refusal that lands before the page re-renders stays: Stop desk's gglib…", async () => {
    applyRemoteStatus(DESK);
    transport.killRemote.mockRejectedValue(new Error('desk refused the shutdown'));
    renderApp('/devices');
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'More for desk' }));
    await user.click(screen.getByRole('menuitem', { name: "Stop desk's gglib…" }));
    await user.click(await screen.findByRole('button', { name: 'Stop it' }));
    expect(await screen.findByText('desk refused the shutdown')).toBeInTheDocument();
  });

  it("an invite's refusal goes when a join runs, and the join's own refusal stays", async () => {
    applyRemoteStatus(SHARED);
    transport.inviteRemote.mockRejectedValue(new Error('a code is already on offer'));
    transport.joinRemote.mockRejectedValue(new Error('the pairing code was already used'));
    renderApp('/devices');
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Invite a device' }));
    await screen.findByText('a code is already on offer');
    await user.type(field(), `${TICKET}-483920`);
    await user.click(join());
    expect(await screen.findByText('the pairing code was already used')).toBeInTheDocument();
    expect(screen.queryByText('a code is already on offer')).not.toBeInTheDocument();
  });
});
