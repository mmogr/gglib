/**
 * Paired devices: every device this Mac has issued a key to, and the way to
 * retire one.
 *
 * The rows come off the status the place already reads (the roster is a
 * settings field the daemon reads to answer it), so the list cannot disagree
 * with the sharing state above it, and each row prints the daemon's
 * `description` (`device_line.rs`, the words `gglib remote list` prints).
 * Shown whether or not this Mac is shared: a laptop is lost at a moment
 * nobody chose. The wire has no per-device reach, so a row has no dot.
 *
 * Under the list: the tool servers paired devices may use (the status's
 * `mcp_servers`, set in Settings › Tools) while shared, and this Mac's ticket
 * fingerprint with how many peers are connected.
 *
 * @module places/devices/PairedList
 */

import { FC, useCallback } from 'react';
import { useLocation, useNavigate } from 'react-router';
import { MoreHorizontal } from 'lucide-react';
import { settingsAt } from '../../app/paths';
import { Button } from '../../components/ui/Button';
import { Icon } from '../../components/ui/Icon';
import { useConfirmContext } from '../../contexts/ConfirmContext';
import { useToastContext } from '../../contexts/ToastContext';
import { refreshRemoteStatus } from '../../services/remoteEvents';
import { getTransport } from '../../services/transport';
import type { RemoteDevice, RemoteStatus } from '../../services/transport/types/remote';
import { formatError } from '../../utils/errors';
import { Group, Note } from '../library/Group';
import { Menu } from '../library/Menu';

/**
 * What a row is called: its label, else its id; with the id beside the label
 * when another row has the same label, since a device names itself and this
 * is the screen for telling them apart.
 */
export function deviceName(device: RemoteDevice, devices: RemoteDevice[]): string {
  if (!device.label) return device.id;
  const shared = devices.some((d) => d.id !== device.id && d.label === device.label);
  return shared ? `${device.label} (${device.id})` : device.label;
}

export const PairedList: FC<{ status: RemoteStatus }> = ({ status }) => {
  const { confirm } = useConfirmContext();
  const { showToast } = useToastContext();
  const navigate = useNavigate();
  const location = useLocation();
  // `?? []`: a daemon from before the roster answers status without one.
  const devices = status.devices ?? [];
  const peers = status.peers.length;

  const forget = useCallback(
    async (device: RemoteDevice, name: string) => {
      const ok = await confirm({
        title: `Forget "${name}"?`,
        // The id, because the label is whatever the device called itself.
        description: `${device.id}: its key stops being admitted from the next request. Every other device is untouched, and this one can be invited again.`,
        confirmLabel: 'Forget',
        variant: 'danger',
      });
      if (!ok) return;
      try {
        const answer = await getTransport().forgetDevice(device.id);
        // Awaited and checked: nothing emits an event for a forget, so a
        // failed read leaves the row on screen, and a success said over it
        // would contradict the list.
        const listed = await refreshRemoteStatus();
        if (!answer.forgotten) {
          // A 200: this machine held nothing under that name.
          showToast(`This machine held no device called ${device.id}.`, 'info');
        } else if (!listed) {
          showToast(`Forgot ${name}, but the device list could not be read again.`, 'info');
        } else {
          showToast(`Forgot ${name}. Every other device is untouched.`, 'success');
        }
      } catch (err) {
        showToast(`Could not forget ${name}: ${formatError(err)}`, 'error');
      }
    },
    [confirm, showToast],
  );

  return (
    <Group label="Paired devices">
      {devices.length === 0 ? (
        <Note>No devices yet. Invite one above.</Note>
      ) : (
        // `role="list"`: WebKit drops list semantics with `list-style: none`.
        <ul role="list" className="m-0 flex list-none flex-col p-0">
          {devices.map((device) => {
            const name = deviceName(device, devices);
            return (
              <li key={device.id} className="flex items-center gap-sm px-md py-xs">
                <span className="w-40 shrink-0 truncate text-sm text-text">{name}</span>
                <span className="min-w-0 flex-1 truncate text-sm text-text-secondary">{device.description}</span>
                <Menu
                  label={<Icon icon={MoreHorizontal} size={14} />}
                  name={`More for ${name}`}
                  iconOnly
                  items={[{ label: 'Forget…', danger: true, onPick: () => void forget(device, name) }]}
                />
              </li>
            );
          })}
        </ul>
      )}
      {status.enabled && (
        <div className="flex items-center gap-sm px-md pt-xs text-xs text-text-secondary">
          <span className="min-w-0 flex-1">
            Tool servers shared with them: {status.mcp_servers.length > 0 ? status.mcp_servers.join(', ') : 'none'}
          </span>
          <Button variant="ghost" size="sm" onClick={() => navigate(settingsAt(location, 'mcp'), { replace: true })}>
            Change ›
          </Button>
        </div>
      )}
      {status.enabled && status.ticket_fingerprint && (
        <p className="m-0 px-md pt-xs font-mono text-xs text-text-secondary">
          Fingerprint {status.ticket_fingerprint} · {peers} {peers === 1 ? 'peer' : 'peers'}
        </p>
      )}
    </Group>
  );
};
