import React from 'react';
import { Skeleton, Stack, Row } from '../primitives';

/**
 * Skeleton placeholder for a single model list row.
 * Mirrors the ModelsListContent row: name line + param/arch/quant badges.
 */
const SkeletonRow: React.FC = () => (
  <div className="py-md px-base border-b border-border w-full">
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
 */
export const ModelListSkeleton: React.FC = () => (
  <div aria-hidden="true">
    {Array.from({ length: 6 }, (_, i) => (
      <SkeletonRow key={i} />
    ))}
  </div>
);
