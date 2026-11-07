/**
 * Recent: the last five requests, newest first: when, which model, the
 * tokens it wrote and how fast, and what gglib did to it in the proxy's
 * words. Not drawn before the first request. All › opens the live numbers
 * (the dashboard dialog, until Diagnostics › Live).
 *
 * @module places/endpoint/Recent
 */

import type { FC } from 'react';
import { Button } from '../../components/ui/Button';
import type { DashboardSnapshot } from '../../services/transport/types/dashboard';
import { Group } from '../library/Group';
import { recentRows } from './lines';

export const Recent: FC<{ snapshot: DashboardSnapshot; onAll: () => void }> = ({ snapshot, onAll }) => {
  const rows = recentRows(snapshot);
  if (rows.length === 0) return null;
  return (
    <Group
      label="Recent"
      aside={
        <Button variant="link" size="sm" className="text-xs font-normal" onClick={onAll}>
          All ›
        </Button>
      }
    >
      <ul aria-label="Recent requests" className="m-0 flex list-none flex-col p-0">
        {rows.map((r) => (
          <li key={r.key} className="flex items-baseline gap-md px-md py-xs text-xs text-text-secondary tabular-nums">
            <span className="shrink-0">{r.time}</span>
            <span className="min-w-0 truncate text-text">{r.model}</span>
            {r.figures && <span className="shrink-0">{r.figures}</span>}
            <span className="flex-1" />
            {r.done && <span className="shrink-0 text-text">{r.done}</span>}
          </li>
        ))}
      </ul>
    </Group>
  );
};
