import React from 'react';
import { Skeleton, Stack, Row } from '../primitives';

/**
 * Skeleton placeholder for a single HuggingFace model card.
 * Mirrors the ModelCard layout: model name + id line on the left,
 * parameter badge + stats on the right.
 */
export const ModelCardSkeleton: React.FC = () => (
  <div
    className="bg-surface-elevated border border-border rounded-xl overflow-hidden"
    aria-hidden="true"
  >
    <div className="px-4 py-[0.9rem]">
      <Row gap="base" align="start" justify="between">
        <Stack gap="xs" className="flex-1 min-w-0">
          {/* Model name */}
          <Skeleton variant="text" width="55%" height="1.1em" />
          {/* Model ID (mono, smaller) */}
          <Skeleton variant="text" width="75%" height="0.8em" />
        </Stack>
        {/* Right: param count badge + downloads + likes */}
        <Row gap="sm" className="shrink-0">
          <Skeleton width="36px" height="1.5em" />
          <Skeleton width="48px" height="0.875em" />
          <Skeleton width="40px" height="0.875em" />
        </Row>
      </Row>
    </div>
  </div>
);
