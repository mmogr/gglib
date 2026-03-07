import React from 'react';
import { Skeleton, Stack } from '../primitives';

/**
 * Skeleton placeholder for a single conversation list item.
 * Mirrors the ConversationListPanel button: title line + relative timestamp.
 */
const SkeletonItem: React.FC = () => (
  <div className="py-md px-base border border-border rounded-base">
    <Stack gap="xs">
      <Skeleton variant="text" width="70%" />
      <Skeleton variant="text" width="35%" height="0.75em" />
    </Stack>
  </div>
);

/**
 * Full-panel skeleton for the conversation list.
 * Replaces the "Loading conversations…" text while conversations are being fetched.
 * Rendered inside the flex-1 overflow container of ConversationListPanel.
 */
export const ConversationListSkeleton: React.FC = () => (
  <div className="flex flex-col gap-sm" aria-hidden="true">
    {Array.from({ length: 5 }, (_, i) => (
      <SkeletonItem key={i} />
    ))}
  </div>
);
