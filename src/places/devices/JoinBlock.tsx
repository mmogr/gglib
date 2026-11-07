/**
 * Use another machine: a loopback port here that is another machine's
 * endpoint.
 *
 * First time, the whole `<ticket>-<code>` string; afterwards Reconnect dials
 * the stored pairing with the stored key, offered only while both are held.
 * Joined, the machine is shown by its name and its reach in the daemon's
 * word, never by its fingerprint (its identity, only compared), with its
 * address and the command that prints the key a client there needs: the key
 * is terminal-only (#1584), so it is never shown here. Its models are in the
 * library; nothing is picked here.
 *
 * @module places/devices/JoinBlock
 */

import { FC, useState } from 'react';
import { MoreHorizontal } from 'lucide-react';
import { CopyValue } from '../../components/CopyValue';
import { REACH_MARK, StatusMark } from '../../components/status/StatusMark';
import { REACH_WORDS } from '../../components/status/words';
import { Button } from '../../components/ui/Button';
import { Icon } from '../../components/ui/Icon';
import { Input } from '../../components/ui/Input';
import { REVEAL_COMMAND } from '../../constants/secrets';
import { useConfirmContext } from '../../contexts/ConfirmContext';
import { useToastContext } from '../../contexts/ToastContext';
import { refreshRemoteStatus } from '../../services/remoteEvents';
import { pairedName } from '../../services/remoteRegistry';
import { getTransport } from '../../services/transport';
import type { RemoteStatus } from '../../services/transport/types/remote';
import { formatError } from '../../utils/errors';
import { Group } from '../library/Group';
import { Menu } from '../library/Menu';
import { awaySince } from './devices';
import { useBlockError, type PageActions } from './pageActions';

const LINE = 'm-0 flex flex-wrap items-center gap-sm text-sm text-text-secondary';

export const JoinBlock: FC<{ status: RemoteStatus; actions: PageActions }> = ({ status, actions }) => {
  const { confirm } = useConfirmContext();
  const { showToast } = useToastContext();
  const [pairing, setPairing] = useState('');
  // Which action is out, so "Reaching it…" stands in for the button pressed.
  const [busy, setBusy] = useState<'join' | 'reconnect' | 'other' | null>(null);
  const [error, setError] = useBlockError(actions, 'join');

  const connected = status.connected;
  const name = pairedName(status);
  // The fingerprint says whether a pairing is stored; the name is what is shown of it.
  const stored = status.stored_ticket_fingerprint !== null;
  const canReuse = stored && status.has_remote_key;

  // Whether the connection went while this page watched: then the line says
  // so, in place of the plain "Last joined".
  const [wasConnected, setWasConnected] = useState(connected !== null);
  const [dropped, setDropped] = useState(false);
  if ((connected !== null) !== wasConnected) {
    setWasConnected(connected !== null);
    setDropped(connected === null);
  }

  const act = async (kind: 'join' | 'reconnect' | 'other', run: () => Promise<unknown>) => {
    actions.onAct('join');
    setBusy(kind);
    setError(null);
    try {
      await run();
      void refreshRemoteStatus();
    } catch (err) {
      setError(formatError(err));
    } finally {
      setBusy(null);
    }
  };

  // Trimmed on send, kept as typed in the field.
  const join = (text: string) =>
    act(text ? 'join' : 'reconnect', async () => {
      await getTransport().joinRemote(text ? { pairing: text } : {});
      setPairing('');
    });

  const stop = async () => {
    const ok = await confirm({
      title: `Stop ${name}'s gglib?`,
      description: `This stops gglib on ${name}: its endpoint, its models, its downloads. Nothing can start it again from here.`,
      confirmLabel: 'Stop it',
      variant: 'danger',
    });
    if (ok) await act('other', () => getTransport().killRemote());
  };

  const failure = error !== null && <p className="m-0 text-xs text-danger">{error}</p>;

  if (connected !== null) {
    const away = status.reach === 'away';
    const reach = away
      ? `${REACH_WORDS.away}${connected.away_for_s === null ? '' : ` since ${awaySince(connected.away_for_s)}`} · reconnects on its own`
      : `${REACH_WORDS[status.reach]} · ${connected.path}`;
    return (
      <Group label="Use another machine">
        <div className="flex flex-col gap-sm px-md">
          {/* The join event carries a port and nothing else: until the read
              names the peer, nobody is named. */}
          {connected.ticket_fingerprint === '' ? (
            <p className={LINE}>{REACH_WORDS[status.reach]}. Reading which machine…</p>
          ) : (
            <StatusMark kind={REACH_MARK[status.reach]} word={`${name} · ${reach}`} className="text-text" />
          )}
          <p className={LINE}>
            <code className="font-mono text-xs text-text">{connected.base_url}</code>
            <CopyValue value={connected.base_url} label="Copy the address" onCopied={() => showToast('Copied.', 'success')} />
          </p>
          <p className={LINE}>
            Clients need this device&apos;s key:
            <code className="font-mono text-xs text-text">{REVEAL_COMMAND.remote_pairing_key}</code>
            <CopyValue
              value={REVEAL_COMMAND.remote_pairing_key}
              label="Copy the command that prints the key"
              onCopied={() => showToast('Copied.', 'success')}
            />
          </p>
          <p className={LINE}>{name}&apos;s models are in your Library.</p>
          <div className="flex items-center gap-sm">
            <Button variant="ghost" size="sm" onClick={() => void act('other', () => getTransport().disconnectRemote())} disabled={busy !== null}>
              Disconnect
            </Button>
            <Menu
              label={<Icon icon={MoreHorizontal} size={14} />}
              name={`More for ${name}`}
              iconOnly
              items={[{ label: `Stop ${name}'s gglib…`, danger: true, onPick: () => void stop() }]}
            />
          </div>
          {failure}
        </div>
      </Group>
    );
  }

  const reconnect = busy === 'reconnect' ? (
    <StatusMark kind="changing" word="Reaching it…" />
  ) : (
    <Button variant="secondary" size="sm" onClick={() => void join('')} disabled={busy !== null}>
      Reconnect
    </Button>
  );

  return (
    <Group label="Use another machine">
      <div className="flex flex-col gap-sm px-md">
        {canReuse && dropped && (
          <p className={LINE}>
            {REACH_WORDS.disconnected}. The pairing is remembered. {reconnect}
          </p>
        )}
        {canReuse && !dropped && (
          <p className={LINE}>
            Last joined {name} {reconnect}
          </p>
        )}
        {stored && !status.has_remote_key && (
          <p className={LINE}>Last joined {name}, but no key is stored here. Paste a new pairing string.</p>
        )}
        <form
          className="flex items-center gap-sm"
          onSubmit={(event) => {
            event.preventDefault();
            if (pairing.trim()) void join(pairing.trim());
          }}
        >
          <Input
            aria-label="Paste a pairing string"
            placeholder="Paste a pairing string"
            className="min-w-0 flex-1 font-mono"
            value={pairing}
            onChange={(event) => {
              setPairing(event.target.value);
              setError(null);
            }}
          />
          {busy === 'join' ? (
            <StatusMark kind="changing" word="Reaching it…" />
          ) : (
            <Button type="submit" variant="secondary" size="sm" disabled={busy !== null || !pairing.trim()}>
              Join
            </Button>
          )}
        </form>
        {failure}
      </div>
    </Group>
  );
};
