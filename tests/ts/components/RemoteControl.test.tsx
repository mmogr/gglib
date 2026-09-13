/**
 * The Remote popover: both sides of the tunnel, and the devices on it.
 *
 * What is pinned here is the shape of the one-time reveal — a pairing exists
 * on a screen only in the answer to `enable --invite` or `invite`, so the
 * panel has to show it the moment it arrives and nowhere else; the connect
 * half's guard rails, where the button is dead with nothing to dial and a
 * stored ticket without a stored key says so instead of failing later; and
 * what a device row may and may not be called, which is the half of this
 * panel that can talk somebody into revoking the wrong machine.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { FC, ReactNode } from 'react';

import RemoteControl from '../../../src/components/RemoteControl';
import type { RemoteDevice } from '../../../src/services/transport/types/remote';
import { ToastProvider, useToastContext } from '../../../src/contexts/ToastContext';
import { ToastContainer } from '../../../src/components/Toast';
import { ConfirmProvider } from '../../../src/contexts/ConfirmContext';
import {
  IDLE_STATUS,
  applyRemoteStatus,
  getRemoteState,
  resetRemoteState,
} from '../../../src/services/remoteRegistry';

const enableRemote = vi.fn();
const connectRemote = vi.fn();
const inviteRemote = vi.fn();
const forgetDevice = vi.fn();

vi.mock('../../../src/services/transport', () => ({
  getTransport: () => ({ enableRemote, connectRemote, inviteRemote, forgetDevice }),
}));
const refreshRemoteStatus = vi.fn(() => Promise.resolve(true));
vi.mock('../../../src/services/remoteEvents', () => ({
  refreshRemoteStatus: () => refreshRemoteStatus(),
}));

/**
 * `ToastProvider` only holds the queue; a separate `ToastContainer` renders
 * it. Without one, every notice this panel gives goes nowhere a test can see
 * — which is how "forgot a device that was already gone" could have claimed
 * success unnoticed.
 */
const Toasts: FC = () => {
  const { toasts, dismissToast } = useToastContext();
  return <ToastContainer toasts={toasts} onDismiss={dismissToast} />;
};

const wrapper = ({ children }: { children: ReactNode }) => (
  <ToastProvider>
    <ConfirmProvider>
      {children}
      <Toasts />
    </ConfirmProvider>
  </ToastProvider>
);

const TICKET = 'pipeadlvvgabqkyqvn6vjp7nhslea45a5yls6pnkmizfv4bbu2hxa5iruaaauhlp2na';

/** A device that redeemed its invite and has made requests since. */
function paired(): RemoteDevice {
  return {
    id: 'dev-0a1b2c3d',
    label: "Matt's phone",
    joined_at: Date.now() - 86_400_000,
    redeemed_at: Date.now() - 86_400_000,
    last_seen: Date.now() - 60_000,
    admitted: true,
  };
}

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
    inviteRemote.mockReset();
    forgetDevice.mockReset();
    refreshRemoteStatus.mockReset().mockResolvedValue(true);
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

    // `invite: true` because no device has joined this machine: the daemon's
    // plain `enable` is a switch and hands out nothing, so a first run would
    // otherwise be two actions. The other half of that predicate is pinned
    // by 'a re-enable on a paired machine…' below.
    expect(enableRemote).toHaveBeenCalledWith({ allow_mcp: false, invite: true });
    expect(await screen.findByText('483920')).toBeInTheDocument();
    expect(screen.getByText(`gglib remote join ${TICKET}-483920`)).toBeInTheDocument();
  });

  it('shows no pairing at all when the daemon answered without a code', async () => {
    // The shape a plain `enable` returns. Rendering it would put an empty
    // code under a countdown reading 0s — a pairing that looks expired,
    // which is the one thing the absence must not be mistaken for.
    enableRemote.mockResolvedValue({ ticket: TICKET });
    const user = await open();
    await user.click(screen.getByRole('button', { name: /enable remote access/i }));

    expect(screen.queryByText(/On the other machine/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/expires in/i)).not.toBeInTheDocument();
  });

  it('a switch that is on with nothing bound says so, and Enable is still there to press', async () => {
    applyRemoteStatus({ ...IDLE_STATUS, remote_enabled: true, enabled: false });
    await open();
    expect(screen.getByText(/nothing is bound yet/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Enable remote access' })).toBeEnabled();
  });

  it('an enable answered by a session that came back does not claim it changed the proxy', async () => {
    applyRemoteStatus({ ...IDLE_STATUS, remote_enabled: true, enabled: false, devices: [paired()] });
    enableRemote.mockResolvedValue({ ticket: TICKET, mcp_allowed: false, already_up: true });
    const user = await open();
    await user.click(screen.getByRole('checkbox', { name: /reach \/mcp/i }));
    await user.click(screen.getByRole('button', { name: 'Enable remote access' }));
    expect(enableRemote).toHaveBeenCalledWith({ allow_mcp: true, invite: false });
    expect(await screen.findByText(/already coming back up/i)).toBeInTheDocument();
    expect(screen.getByText(/the \/mcp box did not take/i)).toBeInTheDocument();
    expect(screen.queryByText(/now requires the API key/i)).not.toBeInTheDocument();
  });

  it('invite shows the same reveal against a tunnel that is already up', async () => {
    // The case that has no command to fall back on: enabled already, so
    // `enable` would 409, and the only other way to add a device was to
    // disable — which drops every device already using the tunnel.
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, ticket_fingerprint: '3ca82708b995' });
    inviteRemote.mockResolvedValue({
      ticket: TICKET,
      code: '119284',
      pairing: `${TICKET}-119284`,
      expires_in_s: 120,
    });
    const user = await open();

    await user.click(screen.getByRole('button', { name: /invite a device/i }));

    // No arguments: `invite` takes no flags, because the session's flags belong
    // to the `enable` that armed it and this must not look like it can
    // change them.
    expect(inviteRemote).toHaveBeenCalledWith();
    expect(enableRemote).not.toHaveBeenCalled();
    expect(await screen.findByText('119284')).toBeInTheDocument();
    expect(screen.getByText(`gglib remote join ${TICKET}-119284`)).toBeInTheDocument();
  });

  it('a daemon whose status has no device list does not take the panel down', async () => {
    // The app adopts whatever daemon is already running, and one from before
    // this build answers status with no `devices` at all. Reading `.some` off
    // that was a TypeError that blanked the whole window the moment the
    // popover opened; the enable path has to treat it as "nobody has joined".
    const older: Record<string, unknown> = { ...IDLE_STATUS };
    delete older.devices;
    applyRemoteStatus(older as unknown as Parameters<typeof applyRemoteStatus>[0]);
    enableRemote.mockResolvedValue({ ticket: TICKET });
    const user = await open();

    expect(screen.getByRole('heading', { name: 'This machine' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /enable remote access/i }));
    expect(enableRemote).toHaveBeenCalledWith({ allow_mcp: false, invite: true });
  });

  it('a device that has made requests is never called one that never joined', async () => {
    // `redeemed_at` is written by a background task and can be lost, and
    // builds of main between #1027 and #1028 wrote roster rows without it —
    // so the row below is what such a device can look like. Calling it "never
    // joined" would invite someone to retire a laptop that works.
    applyRemoteStatus({
      ...IDLE_STATUS,
      enabled: true,
      devices: [
        {
          id: 'dev-0a1b2c3d',
          label: "Matt's laptop",
          joined_at: Date.now() - 86_400_000,
          redeemed_at: null,
          last_seen: Date.now() - 120_000,
          admitted: true,
        },
      ],
    });
    await open();

    expect(screen.getByText("Matt's laptop")).toBeInTheDocument();
    expect(screen.getByText(/last seen 2m ago/i)).toBeInTheDocument();
    expect(screen.queryByText(/never joined/i)).not.toBeInTheDocument();
  });

  it('an invite nobody took says so rather than rendering as a device', async () => {
    applyRemoteStatus({
      ...IDLE_STATUS,
      enabled: true,
      devices: [
        {
          // Omitted, not null: the DTO skips it when absent, so the
          // generated type is `label?: string`.
          joined_at: Date.now() - 3_600_000,
          id: 'dev-99887766',
          redeemed_at: null,
          last_seen: null,
          // Minted and seeded at the edge before the code was ever shown, so
          // admission is not what tells the two apart.
          admitted: true,
        },
      ],
    });
    await open();

    expect(screen.getByText(/invited 1h ago, never joined/i)).toBeInTheDocument();
  });

  it('an invite the edge no longer admits says both, as the CLI does', async () => {
    applyRemoteStatus({
      ...IDLE_STATUS,
      enabled: true,
      devices: [
        {
          joined_at: Date.now() - 3_600_000,
          id: 'dev-99887766',
          redeemed_at: null,
          last_seen: null,
          // What a half-unwound invite leaves: a row the edge has dropped.
          admitted: false,
        },
      ],
    });
    await open();

    expect(screen.getByText(/invited 1h ago, never joined · not admitted/i)).toBeInTheDocument();
  });

  it('a row says which state it is in, and never mistakes one for another', async () => {
    // Three states a redeemed row can be in; the CLI's own tests pin two of
    // them, tunnel down and no requests yet. `admitted: null` is the one that
    // matters most: with the tunnel down nothing is admitted, so a row reading
    // "not admitted" would name one device as the one that was dropped.
    applyRemoteStatus({
      ...IDLE_STATUS,
      devices: [
        {
          id: 'dev-11111111',
          label: 'Down',
          joined_at: Date.now() - 5_000,
          redeemed_at: Date.now() - 5_000,
          last_seen: Date.now() - 5_000,
          admitted: null,
        },
        {
          id: 'dev-22222222',
          label: 'Dropped',
          joined_at: Date.now() - 5_000,
          redeemed_at: Date.now() - 5_000,
          last_seen: Date.now() - 5_000,
          admitted: false,
        },
        {
          id: 'dev-33333333',
          label: 'Fresh',
          joined_at: Date.now() - 5_000,
          redeemed_at: Date.now() - 5_000,
          last_seen: null,
          admitted: true,
        },
      ],
    });
    await open();

    expect(screen.getByText(/tunnel down/)).toBeInTheDocument();
    expect(screen.getByText(/not admitted/)).toBeInTheDocument();
    expect(screen.getByText(/no requests yet/)).toBeInTheDocument();
    // The tunnel-down row must not also read as a retirement.
    expect(screen.getAllByText(/not admitted/)).toHaveLength(1);
  });

  it('two devices that named themselves the same thing are still two devices', async () => {
    // A device names itself, so nothing stops two from choosing one name.
    // This is the screen for deciding which to revoke; if the rows, their
    // buttons and their confirms are identical, it cannot do its job.
    const row = {
      joined_at: Date.now() - 5_000,
      redeemed_at: Date.now() - 5_000,
      last_seen: Date.now() - 5_000,
      admitted: true,
    };
    applyRemoteStatus({
      ...IDLE_STATUS,
      devices: [
        { ...row, id: 'dev-aaaaaaaa', label: 'iPhone' },
        { ...row, id: 'dev-bbbbbbbb', label: 'iPhone' },
      ],
    });
    await open();

    expect(screen.getByText('dev-aaaaaaaa')).toBeInTheDocument();
    expect(screen.getByText('dev-bbbbbbbb')).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Forget iPhone (dev-aaaaaaaa)' }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Forget iPhone (dev-bbbbbbbb)' }),
    ).toBeInTheDocument();
  });

  it('cancelling the confirm retires nothing', async () => {
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, devices: [paired()] });
    const user = await open();

    await user.click(screen.getByRole('button', { name: "Forget Matt's phone (dev-0a1b2c3d)" }));
    await user.click(await screen.findByRole('button', { name: /cancel/i }));

    expect(forgetDevice).not.toHaveBeenCalled();
    expect(screen.getByText("Matt's phone")).toBeInTheDocument();
  });

  it('a forget of something already gone says so rather than claiming success', async () => {
    // `forgotten: false` is a 200, not a 404 — the outcome asked for is that
    // no key is held under that name, and none is. Announcing "Forgot it"
    // would tell someone a revocation happened that did not.
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, devices: [paired()] });
    forgetDevice.mockResolvedValue({ forgotten: false });
    const user = await open();

    await user.click(screen.getByRole('button', { name: "Forget Matt's phone (dev-0a1b2c3d)" }));
    await user.click(screen.getByRole('button', { name: /^forget$/i }));

    expect(await screen.findByText(/held no device called dev-0a1b2c3d/)).toBeInTheDocument();
  });

  it('a re-enable on a paired machine does not mint an invite nobody asked for', async () => {
    // Every invite writes a device id, a key and a roster row, redeemed or
    // not, and nothing sweeps the unredeemed. `invite: true` on every enable
    // meant one more "never joined" row per Disable→Enable, forever, plus a
    // live pairing grant nobody was watching.
    applyRemoteStatus({ ...IDLE_STATUS, devices: [paired()] });
    enableRemote.mockResolvedValue({ ticket: TICKET });
    const user = await open();

    await user.click(screen.getByRole('button', { name: /enable remote access/i }));

    expect(enableRemote).toHaveBeenCalledWith({ allow_mcp: false, invite: false });
  });

  it('a device seen but never stamped redeemed counts as joined on enable', async () => {
    // `redeemed_at` can be lost, and `last_seen` is the second opinion — so a
    // row with requests on it is a device that arrived, and a re-enable must
    // not mint an invite for it.
    applyRemoteStatus({ ...IDLE_STATUS, devices: [{ ...paired(), redeemed_at: null }] });
    enableRemote.mockResolvedValue({ ticket: TICKET });
    const user = await open();
    await user.click(screen.getByRole('button', { name: /enable remote access/i }));
    expect(enableRemote).toHaveBeenCalledWith({ allow_mcp: false, invite: false });
  });

  it('a machine whose only row is an invite nobody took still asks for a code on enable', async () => {
    // Counting rows would call this machine paired on the strength of a code
    // that expired, and then offer no pairing on the one enable that needs it.
    applyRemoteStatus({
      ...IDLE_STATUS,
      devices: [{ ...paired(), redeemed_at: null, last_seen: null }],
    });
    enableRemote.mockResolvedValue({ ticket: TICKET });
    const user = await open();
    await user.click(screen.getByRole('button', { name: /enable remote access/i }));
    expect(enableRemote).toHaveBeenCalledWith({ allow_mcp: false, invite: true });
  });

  it('forget asks first, and says every other device is untouched', async () => {
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, devices: [paired()] });
    forgetDevice.mockResolvedValue({ forgotten: true });
    const user = await open();

    await user.click(screen.getByRole('button', { name: "Forget Matt's phone (dev-0a1b2c3d)" }));

    // Nothing is retired on the click alone: this is the one irreversible
    // thing the panel can do to a device.
    expect(forgetDevice).not.toHaveBeenCalled();
    expect(await screen.findByText(/Forget "Matt's phone"\?/)).toBeInTheDocument();
    // The confirm says what a forget leaves alone, before anything is retired.
    expect(screen.getByText(/Every other device is untouched/)).toBeInTheDocument();

    // The dialog's own button, which is the only bare "Forget" on screen.
    await user.click(screen.getByRole('button', { name: /^forget$/i }));
    expect(forgetDevice).toHaveBeenCalledWith('dev-0a1b2c3d');
    // "Untouched", the CLI's word, not "still connected": a forget works with
    // the tunnel down, when nothing is connected at all.
    expect(
      await screen.findByText("Forgot Matt's phone. Every other device is untouched."),
    ).toBeInTheDocument();
  });

  it('an expired code re-reads the status, so Invite comes back', async () => {
    // `pairing_active` is time-based on the daemon and clears only when
    // something reads it, and nothing emits an event when a code lapses. The
    // reveal leaving is therefore the only moment this panel learns the code
    // is gone — and Invite is disabled while one is live, so without the
    // re-read the button stays dead on the exact path someone takes to ask
    // for another code.
    // Enabled with no code live, which is what makes Invite clickable.
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true });
    inviteRemote.mockResolvedValue({
      ticket: TICKET,
      code: '119284',
      pairing: `${TICKET}-119284`,
      expires_in_s: 1,
    });
    const user = await open();
    refreshRemoteStatus
      .mockImplementationOnce(() => {
        // The read after the invite: the code is live, which is what holds
        // Invite dead until something reads the status again.
        applyRemoteStatus({ ...IDLE_STATUS, enabled: true, pairing_active: true });
        return Promise.resolve(true);
      })
      .mockImplementationOnce(() => {
        // The read on lapse: the daemon no longer holds the code.
        applyRemoteStatus({ ...IDLE_STATUS, enabled: true });
        return Promise.resolve(true);
      });
    await user.click(screen.getByRole('button', { name: /invite a device/i }));
    await screen.findByText('119284');

    // The countdown reaches zero, the reveal takes itself off, and the re-read
    // it asks for is what brings Invite back.
    await waitFor(
      () => expect(screen.getByRole('button', { name: /invite a device/i })).toBeEnabled(),
      { timeout: 3000 },
    );
    expect(screen.queryByText('119284')).not.toBeInTheDocument();
    expect(screen.getByText('no code live')).toBeInTheDocument();
  });

  it('forgetting the invited device takes its code off screen and gives Invite back', async () => {
    // Forgetting the row an invite wrote is the only way to cancel one from
    // here, and it withdraws the code on the daemon. No event says so; the
    // panel learns it from the re-read the forget already does, and has to
    // act on it — or it shows a code nobody can redeem and holds Invite dead
    // for the rest of the countdown.
    const invited: RemoteDevice = {
      id: 'dev-99887766',
      joined_at: Date.now(),
      redeemed_at: null,
      last_seen: null,
      admitted: true,
    };
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true });
    inviteRemote.mockResolvedValue({
      ticket: TICKET,
      code: '119284',
      pairing: `${TICKET}-119284`,
      expires_in_s: 120,
    });
    forgetDevice.mockResolvedValue({ forgotten: true });
    const user = await open();
    refreshRemoteStatus
      .mockImplementationOnce(() => {
        // The read after the invite: the code is live and its row is listed.
        applyRemoteStatus({
          ...IDLE_STATUS,
          enabled: true,
          pairing_active: true,
          devices: [invited],
        });
        return Promise.resolve(true);
      })
      .mockImplementationOnce(() => {
        // The read after the forget: the row is gone, and the code with it.
        applyRemoteStatus({ ...IDLE_STATUS, enabled: true });
        return Promise.resolve(true);
      });

    await user.click(screen.getByRole('button', { name: /invite a device/i }));
    await screen.findByText('119284');
    await user.click(screen.getByRole('button', { name: 'Forget dev-99887766' }));
    await user.click(await screen.findByRole('button', { name: /^forget$/i }));

    await waitFor(() => expect(screen.queryByText('119284')).not.toBeInTheDocument());
    expect(screen.getByRole('button', { name: /invite a device/i })).toBeEnabled();
  });

  it('a fresh code is not dropped because the status has not caught up with it', async () => {
    // The status says no code is live until the read after an invite lands,
    // so a reveal that left on "nothing is live" would take every new code
    // straight off screen. Only a read that had already shown the code live
    // may drop it — and a first code that lapsed must not change that. This
    // pins the shape of the fix as much as the bug.
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true });
    inviteRemote
      .mockResolvedValueOnce({
        ticket: TICKET,
        code: '111111',
        pairing: `${TICKET}-111111`,
        expires_in_s: 1,
      })
      .mockResolvedValueOnce({
        ticket: TICKET,
        code: '222222',
        pairing: `${TICKET}-222222`,
        expires_in_s: 120,
      });
    const user = await open();
    refreshRemoteStatus
      .mockImplementationOnce(() => {
        applyRemoteStatus({ ...IDLE_STATUS, enabled: true, pairing_active: true });
        return Promise.resolve(true);
      })
      .mockImplementationOnce(() => {
        applyRemoteStatus({ ...IDLE_STATUS, enabled: true });
        return Promise.resolve(true);
      });

    await user.click(screen.getByRole('button', { name: /invite a device/i }));
    await screen.findByText('111111');
    await waitFor(
      () => expect(screen.getByRole('button', { name: /invite a device/i })).toBeEnabled(),
      { timeout: 3000 },
    );

    // The second invite's own read applies nothing here: the status still
    // says no code is live, exactly as it does in the real gap.
    await user.click(screen.getByRole('button', { name: /invite a device/i }));
    expect(await screen.findByText('222222')).toBeInTheDocument();
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(screen.getByText('222222')).toBeInTheDocument();
  });

  it('a code this panel did not show still gives Invite back when it lapses', async () => {
    // Offered from a terminal, or shown before the popover was last closed:
    // there is no reveal here to expire, no event when the code lapses, and
    // `pairing_active` clears only when something reads it. Without reads of
    // its own the panel would hold Invite dead until it was reopened.
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      applyRemoteStatus({ ...IDLE_STATUS, enabled: true, pairing_active: true });
      render(<RemoteControl />, { wrapper });
      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
      await user.click(screen.getByRole('button', { name: /remote/i }));
      expect(screen.getByRole('button', { name: /invite a device/i })).toBeDisabled();

      refreshRemoteStatus.mockImplementation(() => {
        applyRemoteStatus({ ...IDLE_STATUS, enabled: true });
        return Promise.resolve(true);
      });
      await act(() => vi.advanceTimersByTimeAsync(120_000));

      expect(screen.getByRole('button', { name: /invite a device/i })).toBeEnabled();
    } finally {
      vi.useRealTimers();
    }
  });

  it('a forget whose re-read fails does not claim the list is current', async () => {
    // The refresh never rejects: awaiting it orders the work, and only its
    // answer says whether the read landed. Nothing else would tell — no event
    // is emitted for a forget — so announcing the forget as done over a list
    // that still shows this one would be the panel contradicting itself.
    applyRemoteStatus({ ...IDLE_STATUS, enabled: true, devices: [paired()] });
    forgetDevice.mockResolvedValue({ forgotten: true });
    refreshRemoteStatus.mockResolvedValue(false);
    const user = await open();

    await user.click(screen.getByRole('button', { name: "Forget Matt's phone (dev-0a1b2c3d)" }));
    await user.click(screen.getByRole('button', { name: /^forget$/i }));

    expect(await screen.findByText(/could not be re-read/)).toBeInTheDocument();
    // Only that notice: the plain success must not be shown beside it.
    expect(screen.queryByText(/Every other device is untouched\./)).not.toBeInTheDocument();
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

  it('a connection that has not named its peer yet does not name one', async () => {
    // The window between `remote_connected` and the status read: a port is
    // known and nothing else. The panel used to fill the gap with whichever
    // ticket happened to be stored, which on a dial to a new machine names
    // the machine being left.
    applyRemoteStatus({
      ...IDLE_STATUS,
      stored_ticket_fingerprint: 'aabbccddeeff',
      connected: {
        port: 41234,
        base_url: 'http://127.0.0.1:41234/v1',
        ticket_fingerprint: '',
        path: 'idle',
        away_for_s: null,
      },
    });
    await open();

    expect(screen.getByText(/reading which machine/i)).toBeInTheDocument();
    expect(screen.queryByText(/aabbccddeeff/)).not.toBeInTheDocument();
  });

  it('the connected half asks which model that machine should be asked for', async () => {
    applyRemoteStatus({
      ...IDLE_STATUS,
      connected: {
        port: 41234,
        base_url: 'http://127.0.0.1:41234/v1',
        ticket_fingerprint: '3ca82708b995',
        path: 'direct',
        away_for_s: null,
      },
    });
    const user = await open();

    // Checking the box with no name says so, rather than letting the turn go
    // and come back `404 Model '' not found` from two machines away.
    await user.click(screen.getByRole('checkbox', { name: /use it for chat/i }));
    expect(screen.getByText(/name one before sending/i)).toBeInTheDocument();

    await user.type(screen.getByLabelText(/model on that machine/i), 'qwen3');
    expect(getRemoteState().chatModel).toBe('qwen3');
    expect(screen.queryByText(/name one before sending/i)).not.toBeInTheDocument();
  });

  it('chat on that machine is dead until a model is named, then routes and asks', async () => {
    applyRemoteStatus({
      ...IDLE_STATUS,
      connected: {
        port: 41234,
        base_url: 'http://127.0.0.1:41234/v1',
        ticket_fingerprint: '3ca82708b995',
        path: 'direct',
        away_for_s: null,
      },
    });
    const user = await open();

    // Nothing named, nothing to open: the far machine answers to its own
    // names and this side has no catalog to guess one from.
    const openChat = screen.getByRole('button', { name: /chat on that machine/i });
    expect(openChat).toBeDisabled();
    expect(getRemoteState().chatRequestedAt).toBeNull();

    await user.type(screen.getByLabelText(/model on that machine/i), 'qwen3');
    expect(openChat).toBeEnabled();
    await user.click(openChat);

    // Both halves of the one decision: the turns are routed there and the
    // page is asked for the screen to type them into.
    expect(getRemoteState().useForChat).toBe(true);
    expect(getRemoteState().chatRequestedAt).toEqual(expect.any(Number));
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
