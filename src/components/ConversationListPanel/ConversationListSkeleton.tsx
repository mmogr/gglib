import React from 'react';
import { Skeleton, Stack } from '../primitives';

/**
 * Skeleton placeholder for a single conversation list item.
 * Mirrors the ConversationListPanel button: title line + relative timestamp.
 */
const SkeletonItem: React.FC = () => (
  <div className="py-sm px-md">
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
  <>
    {/* The shimmer is decorative and hidden from assistive tech, but it
        replaced a literal "Loading conversations…" string that screen readers
        announced — so the announcement is kept explicitly. */}
    <span className="sr-only" role="status">
      Loading conversations…
    </span>
    <div className="flex flex-col gap-sm" aria-hidden="true">
      {Array.from({ length: 5 }, (_, i) => (
        <SkeletonItem key={i} />
      ))}
    </div>
  </>
);
