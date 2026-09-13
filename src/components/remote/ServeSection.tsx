import { FC, useCallback, useEffect, useRef, useState } from 'react';
import { getTransport } from '../../services/transport';
import { useRemoteState } from '../../services/remoteRegistry';
import { refreshRemoteStatus } from '../../services/remoteEvents';
import { formatError } from '../../utils/errors';
import { Button } from '../ui/Button';
import { Checkbox } from '../ui/Checkbox';
import { Label, Stack } from '../primitives';
import { ProxyStatusPill } from '../proxy';
import { PairingReveal } from './PairingReveal';
import type { Pairing } from './PairingReveal';

interface ServeSectionProps {
  onNotice: (message: string, kind: 'success' | 'error' | 'info') => void;
}

/**
 * This machine as the desktop: the tunnel in front of its own proxy.
 *
 * `Enable` brings the tunnel up, and on a machine no device has joined yet
 * it also offers a pairing — so a first run is one action — shown once in a
 * `PairingReveal` that leaves when a device pairs or the code expires.
 * `Invite` shows the same reveal for one more device, against a tunnel that
 * is already up, and changes nothing else about the session. The `/mcp` grant
 * is a checkbox that defaults off, because a leaked key with a shell MCP
 * server configured is remote code execution (ADR 0012).
 *
 * The reveal's lifecycle carries both: it drops on a `paired` transition, and
 * `paired` is per *code* rather than per session, so a second invite clears it
 * and the transition happens again for each device. It also drops when the
 * daemon stops holding the code first — forgetting the row an invite wrote
 * withdraws its code.
 */
export const ServeSection: FC<ServeSectionProps> = ({ onNotice }) => {
  const { status } = useRemoteState();
  const [allowMcp, setAllowMcp] = useState(false);
  // Which action is in flight, not merely that one is: with a single
  // boolean, clicking Invite relabelled Disable as "Stopping…".
  const [busy, setBusy] = useState<'enable' | 'invite' | 'disable' | null>(null);
  const [reveal, setReveal] = useState<Pairing | null>(null);

  const enabled = status?.enabled ?? false;
  const paired = status?.paired ?? false;
  const pairingActive = status?.pairing_active ?? false;
  // The switch on with nothing bound: the daemon is putting the tunnel back
  // after a start, or tried and could not, or another enable is arming. These
  // look the same from here,
  // so Enable stays live. Pressed now it waits for the first, and after the
  // second arms the tunnel with the settings below, where a dead button
  // would be a dead end.
  const comingBack = (status?.remote_enabled ?? false) && !enabled;
  // Whether any device has actually *arrived*, not merely whether the roster
  // has rows in it. An invite nobody took leaves a row behind, so counting
  // rows would call a machine "paired" on the strength of a code that
  // expired — and then neither offer a pairing on enable nor admit that it
  // had not.
  //
  // `?? []`, not only `status?.`: a daemon from before this build answers
  // status with no `devices` at all, and the app adopts whatever daemon is
  // already running. An unguarded `.some` there is a TypeError that takes
  // the whole window down; `DevicesSection` reads the list the same way.
  const anyJoined = (status?.devices ?? []).some(
    (d) => d.redeemed_at !== null || d.last_seen !== null,
  );

  // The reveal leaves the moment a device pairs, when the tunnel goes down
  // under it (a `gglib remote disable` in a terminal), or when the daemon
  // stops holding its code — a forget of the invited row withdraws it, and a
  // reveal left up would show a code nobody can redeem and hold Invite dead
  // for the rest of the countdown.
  //
  // Each of those is a *transition*, not a state. `enable`'s answer arrives
  // before the daemon's event does, and an invite's before the read that
  // follows it, so the reveal must stay on screen through a status that has
  // not caught up yet — one that still says no code is live. Only a read
  // that had already shown the code live can take it away.
  const wasEnabled = useRef(enabled);
  const wasPairingActive = useRef(pairingActive);
  useEffect(() => {
    if (paired || (wasEnabled.current && !enabled) || (wasPairingActive.current && !pairingActive)) {
      setReveal(null);
    }
    wasEnabled.current = enabled;
    wasPairingActive.current = pairingActive;
  }, [paired, enabled, pairingActive]);

  // A code this panel did not put on screen — offered from a terminal, or
  // shown before the popover was last closed — still holds Invite dead while
  // it is live, and nothing says when it lapses: `pairing_active` clears only
  // when something reads it, and no event is emitted. With no reveal of ours
  // to expire, the status is read again every fifteen seconds until a read
  // says the code is gone; a code lives two minutes.
  useEffect(() => {
    if (!pairingActive || reveal) return;
    const poll = window.setInterval(() => void refreshRemoteStatus(), 15_000);
    return () => window.clearInterval(poll);
  }, [pairingActive, reveal]);

  // Asks the daemon again as well as dropping the reveal. `pairing_active`
  // is time-based on the daemon and clears only when something reads it, and
  // no event is emitted when a code lapses — so without this re-read the
  // status keeps saying a code is live, and the Invite button below, which is
  // disabled while one is, stays dead until the panel is closed and reopened.
  // That is the one path someone takes after a code expires.
  const dropReveal = useCallback(() => {
    setReveal(null);
    void refreshRemoteStatus();
  }, []);

  const handleEnable = async () => {
    setBusy('enable');
    try {
      // Only when no device has actually joined yet. An invite is not free:
      // the daemon mints a device id and a key and writes a roster row for
      // every one, redeemed or not, and nothing sweeps the unredeemed. So
      // `invite: true` unconditionally meant each Disable→Enable on a paired
      // machine left one more "invited …, never joined" row behind, plus a
      // live pairing grant nobody was watching — which this panel now
      // displays, one per re-enable, forever.
      //
      // A first run is still one action, because on a first run no device has
      // joined. After that, adding a device is Invite.
      const firstDevice = !anyJoined;
      const answer = await getTransport().enableRemote({
        allow_mcp: allowMcp,
        invite: firstDevice,
      });
      // Present together or not at all; the daemon sends all three or none.
      setReveal(
        answer.pairing && answer.code && answer.expires_in_s !== undefined
          ? {
              pairing: answer.pairing,
              code: answer.code,
              expires_in_s: answer.expires_in_s,
            }
          : null,
      );
      void refreshRemoteStatus();
      // `already_up` here is a session the daemon's own resume brought back
      // while this waited, since Enable is shown only when the last status
      // read had nothing up. This
      // click switched nothing on, so the key notice is not its to give, and
      // a /mcp box that session did not honour says so, as the CLI does.
      const ignoredMcp = answer.already_up && allowMcp && !answer.mcp_allowed;
      onNotice(
        answer.already_up
          ? `Remote access was already coming back up, and is on now.${
              ignoredMcp
                ? ' The /mcp box did not take: that session keeps its own grant; disable and enable again to change it.'
                : ''
            }`
          : 'Remote access is on. The local proxy now requires the API key too.',
        'info',
      );
    } catch (err) {
      onNotice(`Could not enable remote access: ${formatError(err)}`, 'error');
    } finally {
      setBusy(null);
    }
  };

  // Pairing one more device: same reveal, same lifecycle. `invite` changes
  // nothing else about the session — not the flags it was enabled with, not
  // the ticket, not the devices already on it — so there is nothing to
  // confirm and nothing to warn about.
  const handleInvite = async () => {
    setBusy('invite');
    try {
      const answer = await getTransport().inviteRemote();
      setReveal(
        answer.pairing && answer.code && answer.expires_in_s !== undefined
          ? {
              pairing: answer.pairing,
              code: answer.code,
              expires_in_s: answer.expires_in_s,
            }
          : null,
      );
      void refreshRemoteStatus();
    } catch (err) {
      onNotice(`Could not offer a pairing code: ${formatError(err)}`, 'error');
      // A refused offer re-reads the status. A code opened from a terminal
      // emits no event, so a 409 here may be the first this panel hears of
      // one — and the read is what turns this button off until it lapses.
      void refreshRemoteStatus();
    } finally {
      setBusy(null);
    }
  };

  const handleDisable = async () => {
    setBusy('disable');
    try {
      await getTransport().disableRemote();
      setReveal(null);
      onNotice('Remote access is off. Enabling again brings the same ticket back.', 'success');
    } catch (err) {
      onNotice(`Could not disable remote access: ${formatError(err)}`, 'error');
    } finally {
      setBusy(null);
    }
  };

  return (
    <section aria-labelledby="remote-serve-heading">
      <div className="flex justify-between items-center mb-sm">
        <h4 id="remote-serve-heading" className="m-0 text-sm font-semibold text-text">
          This machine
        </h4>
        <ProxyStatusPill running={enabled} />
      </div>

      {reveal && (
        <PairingReveal
          reveal={reveal}
          onExpired={dropReveal}
          onCopied={() => onNotice('Copied.', 'success')}
        />
      )}

      {enabled ? (
        <Stack gap="sm">
          <dl className="grid grid-cols-[auto_1fr] gap-x-md gap-y-xs text-xs m-0">
            <dt className="text-text-muted">Ticket</dt>
            <dd className="m-0 font-mono">{status?.ticket_fingerprint ?? '—'}</dd>
            <dt className="text-text-muted">Pairing</dt>
            <dd className="m-0">
              {/* What the status knows, and nothing it has to guess: whether a
                  code was ever offered this session is not something it says. */}
              {pairingActive
                ? 'code live, waiting for a device'
                : paired
                  ? 'paired'
                  : 'no code live'}
            </dd>
            <dt className="text-text-muted">Peers</dt>
            <dd className="m-0">
              {status?.peers.length
                ? status.peers.map((p) => `${p.fingerprint} (${p.path})`).join(', ')
                : 'none connected'}
            </dd>
            <dt className="text-text-muted">/mcp</dt>
            <dd className="m-0">{status?.mcp_allowed ? 'reachable through the tunnel' : 'not reachable'}</dd>
          </dl>
          <Button
            variant="secondary"
            className="w-full"
            onClick={handleInvite}
            // Dead while the status says a code is live: a second offer is a
            // guaranteed 409, and it is not a cheap one — the daemon mints a
            // key and writes both stores before it finds the slot taken, then
            // unwinds all of it.
            disabled={busy !== null || reveal !== null || pairingActive}
          >
            {busy === 'invite' ? 'Offering…' : 'Invite a device'}
          </Button>
          <Button
            variant="danger"
            className="w-full"
            onClick={handleDisable}
            disabled={busy !== null}
          >
            {busy === 'disable' ? 'Stopping…' : 'Disable remote access'}
          </Button>
        </Stack>
      ) : (
        <Stack gap="sm">
          <Checkbox
            checked={allowMcp}
            onChange={(e) => setAllowMcp(e.target.checked)}
            label="Let the other machine reach /mcp"
            description="Off by default: a leaked key with a shell MCP server configured is remote code execution."
          />
          <Button
            variant="primary"
            className="w-full"
            onClick={handleEnable}
            disabled={busy !== null}
          >
            {busy === 'enable' ? 'Finding a relay…' : 'Enable remote access'}
          </Button>
          <Label size="xs" muted>
            {comingBack
              ? 'Switched on, but nothing is bound yet: the daemon may still be putting it back, or could not. Enabling now waits for it, or arms it again with the settings below. '
              : anyJoined
                ? 'Devices already paired come back on their own; use Invite to add another. '
                : 'Shows a ticket and a six-digit code once, to pair your first device. '}
            Enabling puts the API key on the local proxy too, and disabling does not take that
            away.
          </Label>
        </Stack>
      )}
    </section>
  );
};
