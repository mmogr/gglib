/**
 * Using it: who is reaching the endpoint now.
 *
 * "Apps on this Mac" with how many requests are in flight (the dashboard's
 * active connections): the proxy records no client's identity, so apps
 * are not told apart. Then one line per paired device that has joined,
 * with the daemon's own description of it (`RemoteDevice.description`).
 * With neither, and no request ever served, one sentence and the way to
 * Connect a client.
 *
 * @module places/endpoint/UsingIt
 */

import type { FC } from 'react';
import { Button } from '../../components/ui/Button';
import type { DashboardSnapshot } from '../../services/transport/types/dashboard';
import type { RemoteDevice } from '../../types/generated/RemoteDevice';
import { Group, Note } from '../library/Group';

interface UsingItProps {
  snapshot: DashboardSnapshot;
  devices: RemoteDevice[];
  onConnect: () => void;
}

export const UsingIt: FC<UsingItProps> = ({ snapshot, devices, onConnect }) => {
  const now = snapshot.active_connections.length;
  const joined = devices.filter((d) => d.joined);
  const apps = now > 0 || snapshot.total_requests > 0;
  return (
    <Group label="Using it">
      {!apps && joined.length === 0 && (
        <p className="m-0 flex items-center gap-sm px-md py-xs text-xs text-text-secondary">
          No app has used it yet.
          <Button variant="link" size="sm" className="text-xs font-normal" onClick={onConnect}>
            Connect a client
          </Button>
        </p>
      )}
      {apps && <Note>{`Apps on this Mac · ${now === 0 ? 'no requests now' : `${now} ${now === 1 ? 'request' : 'requests'} now`}`}</Note>}
      {joined.map((d) => (
        <Note key={d.id}>{[d.label ?? d.id, d.description].filter(Boolean).join(' · ')}</Note>
      ))}
    </Group>
  );
};
