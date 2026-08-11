import React from 'react';
import { Skeleton, Stack, Row } from '../primitives';

/**
 * Skeleton placeholder for a single model list row.
 * Mirrors the ModelsListContent row: name line + param/arch/quant badges.
 */
const SkeletonRow: React.FC = () => (
  <div className="py-sm px-md border-l-[3px] border-l-transparent w-full">
    <Stack gap="sm">
      <Skeleton variant="text" width="55%" />
      <Row gap="md" align="center">
        <Skeleton width="40px" height="0.75em" />
        <Skeleton width="60px" height="0.75em" />
        <Skeleton width="36px" height="0.75em" />
      </Row>
    </Stack>
  </div>
);

/**
 * Full-panel skeleton for the model library list.
 * Replaces the "Loading models..." text while the model list is being fetched.
 *
 * The shimmer is decorative, so it is hidden from assistive tech — but it
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
