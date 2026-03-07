import React from 'react';
import { Skeleton, Stack, Row } from '../primitives';

// Mirrors the panelContainer constant in ModelInspectorPanel.tsx
const panelClass =
  'flex flex-col h-full min-h-0 overflow-y-auto overflow-x-hidden relative flex-1 bg-surface';

/**
 * Full-panel skeleton for the ModelInspectorPanel.
 * Shown when models are being initially fetched (loading && !model).
 * Mirrors: header (name + circle action buttons) + metadata rows + tags + action buttons.
 */
export const ModelInspectorSkeleton: React.FC = () => (
  <div className={panelClass} aria-hidden="true">
    {/* Header — mirrors "p-base border-b" bar with model name + icon buttons */}
    <div className="p-base border-b border-border bg-background shrink-0">
      <Row gap="base" justify="between" align="center">
        <Skeleton width="45%" height="1.5rem" />
        <Row gap="xs">
          <Skeleton
            variant="circle"
            width="var(--button-height-base)"
            height="var(--button-height-base)"
          />
          <Skeleton
            variant="circle"
            width="var(--button-height-base)"
            height="var(--button-height-base)"
          />
          <Skeleton
            variant="circle"
            width="var(--button-height-base)"
            height="var(--button-height-base)"
          />
        </Row>
      </Row>
    </div>

    {/* Content — mirrors ModelMetadataGrid + tags + InspectorActions */}
    <div className="flex-1 min-h-0 p-base">
      <Stack gap="xl">
        {/* Model Information section */}
        <Stack gap="md">
          {/* Section heading */}
          <Skeleton width="130px" height="0.7em" />
          {/* label: value rows */}
          {Array.from({ length: 4 }, (_, i) => (
            <Row key={i} gap="base" justify="between" align="start">
              <Skeleton width="80px" height="0.875em" className="shrink-0" />
              <Skeleton width="45%" height="0.875em" />
            </Row>
          ))}
        </Stack>

        {/* Tags section */}
        <Stack gap="sm">
          <Skeleton width="50px" height="0.7em" />
          <Row gap="sm" wrap>
            <Skeleton width="56px" height="1.5rem" className="rounded-full" />
            <Skeleton width="48px" height="1.5rem" className="rounded-full" />
            <Skeleton width="64px" height="1.5rem" className="rounded-full" />
          </Row>
        </Stack>

        {/* Action buttons */}
        <Row gap="base">
          <Skeleton width="100px" height="var(--button-height-base)" />
          <Skeleton width="90px" height="var(--button-height-base)" />
        </Row>
      </Stack>
    </div>
  </div>
);
