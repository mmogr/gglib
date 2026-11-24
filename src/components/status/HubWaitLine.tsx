/**
 * The one line a read or a search the Hub refused shows where its answer or
 * error would be, counting down to when the limit lifts (`Countdown`: the
 * live region announces it once per wait). Waiting is not a failure: it is
 * a status, in the secondary colour.
 *
 * @module components/status/HubWaitLine
 */

import type { FC } from 'react';
import { cn } from '../../utils/cn';
import { Countdown } from './Countdown';
import { liftsLine } from './hubWait';

export const HubWaitLine: FC<{ liftsAt: number; className?: string }> = ({ liftsAt, className }) => (
  <p role="status" className={cn('m-0 text-xs text-text-secondary tabular-nums', className)}>
    <Countdown at={liftsAt} say={liftsLine} />
  </p>
);
