import { FC, useCallback, useState } from 'react';
import { getTransport } from '../../services/transport';
import { useRemoteState } from '../../services/remoteRegistry';
import { refreshRemoteStatus } from '../../services/remoteEvents';
import { useConfirmContext } from '../../contexts/ConfirmContext';
import { formatError } from '../../utils/errors';
import { Button } from '../ui/Button';
import { Label } from '../primitives';
import type { RemoteDevice } from '../../services/transport/types/remote';

interface DevicesSectionProps {
  onNotice: (message: string, kind: 'success' | 'error' | 'info') => void;
}

/**
 * Who may use this machine's tunnel, and the button that retires one.
 *
 * The rows come off the status the popover already reads — the roster is a
 * settings field the daemon reads to answer `status` regardless — so this
 * section has no fetch, no loading state, and no way to disagree with the
 * tunnel state beside it about whether anything is being admitted.
 *
 * That status is re-read when the panel opens and on every tunnel event, not
 * on a timer: nothing fires when `last_seen` is written or when someone types
 * `gglib remote forget` in a terminal, so a panel left open can show a stale
 * roster and a `last seen` that does not tick.
 *
 * Shown whether or not the tunnel is up, and deliberately: a laptop is lost
 * at a moment nobody chose, and a list that appeared only while serving would
 * hide the roster exactly when someone came to change it.
 */
export const DevicesSection: FC<DevicesSectionProps> = ({ onNotice }) => {
  const { status } = useRemoteState();
  const { confirm } = useConfirmContext();
  const [forgetting, setForgetting] = useState<string | null>(null);

  const devices = status?.devices ?? [];

  const handleForget = useCallback(
    async (device: RemoteDevice) => {
      const name = device.label ?? device.id;
      const confirmed = await confirm({
        title: `Forget "${name}"?`,
        description:
          // The id, because the label is whatever the device called itself
          // and two can have called themselves the same thing. This is the
          // last screen before something irreversible.
          `${device.id} — its key stops being admitted from the next request. Every other ` +
          'device is untouched, and this one can be invited again.',
        confirmLabel: 'Forget',
        variant: 'danger',
      });
      if (!confirmed) return;

      setForgetting(device.id);
      try {
        const answer = await getTransport().forgetDevice(device.id);
        // Awaited, so the row is gone from the list before its button comes
        // back rather than a round-trip later, still reading as paired and
        // still offering to be forgotten.
        //
        // And checked, because that read can fail and there is no second
        // chance: nothing emits an event for a forget and nothing polls, so
        // the row would sit there until the panel was reopened. Announcing the
        // forget as done over a list that still shows this one would be the
        // panel contradicting itself.
        const listed = await refreshRemoteStatus();
        if (!listed) {
          onNotice(
            answer.forgotten
              ? `Forgot ${name}, but the device list could not be re-read — reopen this panel.`
              : `This machine held no device called ${device.id}, and the list is out of date.`,
            'info',
          );
          return;
        }
        // `forgotten: false` is a 200: this machine held nothing under that
        // name. Saying so beats a success message about a device that was
        // already gone. "Untouched", not "still connected", and the CLI's
        // word: a forget works with the tunnel down, when nothing is connected.
        onNotice(
          answer.forgotten
            ? `Forgot ${name}. Every other device is untouched.`
            : `This machine held no device called ${device.id}.`,
          answer.forgotten ? 'success' : 'info',
        );
      } catch (err) {
        onNotice(`Could not forget ${name}: ${formatError(err)}`, 'error');
      } finally {
        setForgetting(null);
      }
    },
    [confirm, onNotice],
  );

  return (
    <section aria-labelledby="remote-devices-heading">
      <h4 id="remote-devices-heading" className="m-0 mb-sm text-sm font-semibold text-text">
        Devices
      </h4>

      {devices.length === 0 ? (
        <Label size="xs" muted>
          {status?.enabled
            ? 'No device has been paired with this machine. “Invite a device” offers a code.'
            : 'No device has been paired with this machine. Enabling remote access shows a code that pairs the first one.'}
        </Label>
      ) : (
        <ul role="list" className="list-none m-0 p-0 flex flex-col gap-xs">
          {/* `role="list"` explicitly: WebKit drops list semantics when
              `list-style: none` is set, and those semantics — "three
              devices", announced as a count — are the whole reason this is a
              list rather than a stack of divs. */}
          {devices.map((device) => (
            <li
              key={device.id}
              className="flex justify-between items-center gap-sm py-xs border-b border-border last:border-b-0"
            >
              <div className="min-w-0">
                <div className="text-xs font-medium text-text truncate">
                  {device.label ?? device.id}
                </div>
                {/* The id is always shown, never only as a fallback title.
                    A device names itself, so two can name themselves the
                    same thing — and then the rows, their buttons and their
                    confirm dialogs are identical on the one screen whose
                    whole job is telling them apart. The CLI prints an ID
                    column for the same reason. */}
                <div className="text-xs text-text-muted font-mono truncate">{device.id}</div>
                <div className="text-xs text-text-muted">{describe(device)}</div>
              </div>
              <Button
                type="button"
                variant="dangerGhost"
                size="sm"
                // Named per row: a column of buttons all called "Forget" is
                // a screen reader reading the same word down the list with
                // nothing to tell them apart, on the one action here that
                // cannot be undone.
                aria-label={
                  device.label ? `Forget ${device.label} (${device.id})` : `Forget ${device.id}`
                }
                onClick={() => void handleForget(device)}
                disabled={forgetting !== null}
              >
                {forgetting === device.id ? 'Forgetting…' : 'Forget'}
              </Button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
};

/**
 * The second line of a row: what is known about this device.
 *
 * **A row is only called never-joined when both timestamps are empty.**
 * `redeemed_at` is written by a background task and can be lost, and builds
 * of main between #1027 and #1028 wrote roster rows without it — so a device
 * that has plainly made requests must not be described as one that never
 * arrived. `last_seen` is the second opinion that prevents it.
 */
function describe(device: RemoteDevice): string {
  if (device.redeemed_at === null && device.last_seen === null) {
    return `invited ${ago(device.joined_at)}, never joined`;
  }
  const seen = device.last_seen === null ? 'no requests yet' : `last seen ${ago(device.last_seen)}`;
  if (device.admitted === null) return `${seen} · tunnel down`;
  return device.admitted ? seen : `${seen} · not admitted`;
}

/**
 * A coarse "how long ago", for a line a person scans rather than measures.
 *
 * `trunc`, not `floor`, and a minute of slack: a browser on another host is
 * not in step with the daemon's clock, and a stamp a little ahead is skew,
 * not a clock that moved. Rounding down turned a just-minted invite into
 * "invited at an unknown time".
 *
 * The CLI's `ago` follows the same two rules, so the one roster reads the
 * same on both surfaces. It cannot need the slack — it reads a stamp its own
 * machine wrote — but one rule in two places beats two rules that agree
 * until they do not.
 */
function ago(atMs: number): string {
  const secs = Math.trunc((Date.now() - atMs) / 1000);
  // A clock that really did move backwards. "In the future" is a wrong answer
  // someone can act on; a silent negative is not.
  if (secs < -60) return 'at an unknown time';
  if (secs < 60) return 'just now';
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
  if (secs < 86_400) return `${Math.floor(secs / 3600)}h ago`;
  return `${Math.floor(secs / 86_400)}d ago`;
}
