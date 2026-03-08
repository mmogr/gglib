import React from 'react';
import { Skeleton, Stack, Row } from '../components/primitives';
import { ConversationListSkeleton } from '../components/ConversationListPanel/ConversationListSkeleton';

/**
 * Full-page skeleton for the ChatPage Suspense fallback.
 * Mirrors the two-column layout: conversation list panel (~30%) + message area (~70%).
 * The left column header approximates the real ConversationListPanel header
 * (tabs + model name + search input).
 */
export const ChatPageSkeleton: React.FC = () => (
  <Row gap="none" align="stretch" className="flex-1 min-h-0 bg-background">
    {/* Left panel — conversation list (~30% matches default leftPanelWidth) */}
    <div
      className="flex flex-col h-full min-h-0 shrink-0 border-r border-border overflow-hidden"
      style={{ width: '30%' }}
    >
      {/* Header: tabs + model name + search */}
      <div className="p-base border-b border-border bg-background shrink-0">
        <Stack gap="sm">
          <Row gap="sm">
            <Skeleton width="50%" height="var(--button-height-sm)" />
            <Skeleton width="50%" height="var(--button-height-sm)" />
          </Row>
          <Skeleton variant="text" width="65%" height="1.2em" />
          <Skeleton width="100%" height="var(--input-height-sm)" />
        </Stack>
      </div>

      {/* Conversation items */}
      <div className="flex-1 overflow-y-auto p-xs">
        <ConversationListSkeleton />
      </div>
    </div>

    {/* Right panel — alternating message bubbles (user left, assistant right) */}
    <Stack gap="lg" className="flex-1 h-full min-h-0 p-xl overflow-hidden">
      <Skeleton width="65%" height="56px" className="self-start" />
      <Skeleton width="75%" height="72px" className="self-end" />
      <Skeleton width="60%" height="56px" className="self-start" />
      <Skeleton width="70%" height="72px" className="self-end" />
    </Stack>
  </Row>
);
