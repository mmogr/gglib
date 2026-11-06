import React from 'react';
import { Skeleton, Stack } from '../../components/primitives';

/** One grey two-line row: a name, and the line of facts under it. */
const SkeletonRow: React.FC = () => (
  <div className="min-h-[52px] px-md py-sm">
    <Stack gap="sm">
      <Skeleton variant="text" width="45%" />
      <Skeleton variant="text" width="30%" height="0.75em" />
    </Stack>
  </div>
);

/**
 * The library before its first read: six grey two-line rows, still (a
 * skeleton is a placeholder, not a signal, so it does not move).
 *
 * The placeholder is decorative, so it is hidden from assistive tech — but it
 * replaced a literal "Loading models..." string that screen readers used to
 * announce, so the announcement is carried here explicitly rather than lost
 * with the text.
 */
export const ModelListSkeleton: React.FC = () => (
  <>
    <span className="sr-only" role="status">
      Loading models…
    </span>
    <div aria-hidden="true">
      {Array.from({ length: 6 }, (_, i) => (
        <SkeletonRow key={i} />
      ))}
    </div>
  </>
);
