/**
 * This Mac: the tunnel in front of its own endpoint, and the code that lets
 * one more device in.
 *
 * Share with my devices brings the tunnel up, and on a machine no device has
 * joined yet it also offers a code, so a first run is one action (DV-2); the
 * pairing sheet opens from the answer. Turning it on puts the API key on the
 * endpoint, so it asks first, unless the endpoint already requires one.
 * Invite a device offers one more code against a tunnel already up, and is
 * dead while a code is live. Stop sharing drops the code on offer before it
 * asks, so the event it causes finds nothing to announce.
 *
 * @module places/devices/ShareBlock
 */

import { FC, useEffect, useState } from 'react';
import { useConfirmContext } from '../../contexts/ConfirmContext';
import { StatusMark } from '../../components/status/StatusMark';
import { Button } from '../../components/ui/Button';
import { refreshRemoteStatus } from '../../services/remoteEvents';
import { clearOffer, showOffer, useRemoteState } from '../../services/remoteRegistry';
import { useEndpointFacts } from '../../services/proxyRegistry';
import { getTransport } from '../../services/transport';
import type { RemoteStatus } from '../../services/transport/types/remote';
import { formatError } from '../../utils/errors';
import { Group } from '../library/Group';
import { PairingSheet } from './PairingSheet';

export const ShareBlock: FC<{ status: RemoteStatus }> = ({ status }) => {
  const { offer } = useRemoteState();
  const { key } = useEndpointFacts();
  const { confirm } = useConfirmContext();
  // Which action is in flight, not merely that one is: one boolean once
  // relabelled Stop while an Invite was out.
  const [busy, setBusy] = useState<'enable' | 'invite' | 'disable' | null>(null);
  const [error, setError] = useState<string | null>(null);

  const pairingActive = status.pairing_active;
  // The switch on with nothing bound: the daemon is putting the tunnel back
  // after a start, or tried and could not. Share stays live: pressed now it
  // waits for the first, and arms the tunnel again after the second.
  const comingBack = status.remote_enabled && !status.enabled;
  // Whether any device has arrived, by the daemon's answer for each row: an
  // invite nobody took leaves a row too. `?? []`: a daemon from before the
  // roster answers status with no `devices`, and the app adopts whatever
  // daemon is running.
  const anyJoined = (status.devices ?? []).some((d) => d.joined);

  // A code offered from a terminal holds Invite dead while it is live, and
  // nothing says when it lapses: no event, so only a read finds
  // `pairing_active` gone. Read every fifteen seconds until one does.
  useEffect(() => {
    if (!pairingActive || offer) return;
    const poll = window.setInterval(() => void refreshRemoteStatus(), 15_000);
    return () => window.clearInterval(poll);
  }, [pairingActive, offer]);

  const share = async () => {
    if (key !== 'required') {
      const ok = await confirm({
        title: 'Clients on this Mac will need an API key from now on.',
        confirmLabel: 'Turn on',
      });
      if (!ok) return;
    }
    setBusy('enable');
    setError(null);
    try {
      // A code only while no device has joined: every invite writes a key and
      // a roster row, redeemed or not, and nothing sweeps the unredeemed.
      showOffer(await getTransport().enableRemote({ invite: !anyJoined }));
      void refreshRemoteStatus();
    } catch (err) {
      setError(formatError(err));
    } finally {
      setBusy(null);
    }
  };

  const invite = async () => {
    setBusy('invite');
    setError(null);
    try {
      showOffer(await getTransport().inviteRemote());
    } catch (err) {
      setError(formatError(err));
    } finally {
      setBusy(null);
      // After a refusal too: a code opened from a terminal emits no event, so
      // a 409 may be the first this page hears of one.
      void refreshRemoteStatus();
    }
  };

  const stop = async () => {
    setBusy('disable');
    setError(null);
    // Before the request: its `remote_disabled` can arrive before the answer,
    // and must find no code to announce as ended.
    clearOffer();
    try {
      await getTransport().disableRemote();
    } catch (err) {
      setError(formatError(err));
    } finally {
      setBusy(null);
    }
  };

  return (
    <Group label="This Mac">
      <div className="flex flex-col gap-sm px-md">
        {status.enabled ? (
          <div className="flex flex-wrap items-center gap-sm">
            <StatusMark kind="on" word="Shared with your devices" className="flex-1 text-text" />
            <Button
              variant="primary"
              size="sm"
              onClick={() => void invite()}
              // Dead while a code is live: a second offer is a refusal, and the
              // daemon mints a key and writes both stores before it finds that.
              disabled={busy !== null || offer !== null || pairingActive}
            >
              {busy === 'invite' ? <StatusMark kind="changing" word="Offering…" /> : 'Invite a device'}
            </Button>
            <Button variant="ghost" size="sm" onClick={() => void stop()} disabled={busy !== null}>
              Stop sharing
            </Button>
          </div>
        ) : (
          <>
            <p className="m-0 text-sm text-text-secondary">Let your laptop and phone use this Mac&apos;s models.</p>
            {comingBack && (
              <p className="m-0 text-xs text-text-secondary">
                Switched on, but nothing is bound yet: gglib may still be putting it back, or could not. Sharing now
                waits for it, or arms it again.
              </p>
            )}
            <div>
              {busy === 'enable' ? (
                <StatusMark kind="changing" word="Finding a relay…" />
              ) : (
                <Button variant="primary" size="sm" onClick={() => void share()} disabled={busy !== null}>
                  Share with my devices
                </Button>
              )}
            </div>
          </>
        )}
        {error !== null && <p className="m-0 text-xs text-danger">{error}</p>}
      </div>
      <PairingSheet onNewCode={() => void invite()} offering={busy === 'invite'} />
    </Group>
  );
};
